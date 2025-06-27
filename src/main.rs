extern crate log;

use std::fs::read_to_string;
use std::hash::Hash;
use std::io::{BufRead, BufReader};
use std::net::IpAddr;
use std::process::{self, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use clap::Parser;
use colored::*;
use dns_lookup::lookup_host;
use env_logger::Env;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::{error, info, warn};
use question::{Answer, Question};
use rayon::prelude::*;
use regex::Regex;

// Global state to track the currently open block
lazy_static! {
    static ref CURRENT_BLOCK: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

// Define command-line arguments using the clap library
#[derive(Parser, Debug)]
#[command(author = "AB ab@hexor.ru", version, about = "Parallel SSH executor in Rust", long_about = None)]
struct Args {
    /// Username for SSH connections (defaults to current system user)
    #[arg(short = 'u', short_alias = 'l', long, default_value_t = whoami::username())]
    username: String,

    /// Flag to use known_hosts file for server discovery instead of pattern expansion
    #[arg(
        short,
        long,
        help = "Use known_hosts to build servers list instead of string expansion."
    )]
    known_hosts: bool,

    /// Server name patterns with expansion syntax
    /// Examples: 'web-[1:12]-io-{prod,dev}' expands to multiple servers
    #[arg(
        short,
        long,
        num_args = 1..,
        help = "Expression to build server list. List and range expansion are supported. Example: 'web-[1:12]-io-{prod,dev}'"
    )]
    expression: Vec<String>,

    /// Command to execute on each server
    #[arg(short, long, help = "Command to execute on servers")]
    command: String,

    /// Display only exit codes without command output
    #[arg(long, default_value_t = false, help = "Show exit code ONLY")]
    code: bool,

    /// Skip confirmation prompt before executing commands
    #[arg(
        short = 'f',
        long,
        default_value_t = false,
        help = "Don't ask for confirmation"
    )]
    noconfirm: bool,

    /// Maximum number of parallel SSH connections
    #[arg(short, long, default_value_t = 100)]
    parallel: i32,
}

/// Host representation for both known_hosts entries and expanded patterns
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Host {
    /// Hostname or IP address as a string
    name: String,
    /// Resolved IP address (if available)
    ip: Option<IpAddr>,
}

/// Find common domain suffix across all hostnames to simplify output display
///
/// This function analyzes all hostnames to identify a common domain suffix
/// which can be shortened during display to improve readability.
///
/// # Arguments
/// * `hostnames` - A slice of strings containing all server hostnames
///
/// # Returns
/// * `Option<String>` - The common suffix if found, or None
fn find_common_suffix(hostnames: &[String]) -> Option<String> {
    if hostnames.is_empty() {
        return None;
    }

    // Don't truncate if only one host
    if hostnames.len() == 1 {
        return None;
    }

    let first = &hostnames[0];

    // Start with assumption that the entire first hostname is the common suffix
    let mut common = first.clone();

    // Iterate through remaining hostnames, reducing the common part
    for hostname in hostnames.iter().skip(1) {
        // Exit early if no common part remains
        if common.is_empty() {
            return None;
        }

        // Find common suffix with current hostname
        let mut new_common = String::new();

        // Search for common suffix by comparing characters from right to left
        let mut common_chars = common.chars().rev();
        let mut hostname_chars = hostname.chars().rev();

        loop {
            match (common_chars.next(), hostname_chars.next()) {
                (Some(c1), Some(c2)) if c1 == c2 => new_common.insert(0, c1),
                _ => break,
            }
        }

        common = new_common;
    }

    // Ensure the common part is a valid domain suffix (starts with a dot)
    if common.is_empty() || !common.starts_with('.') {
        return None;
    }

    // Return the identified common suffix
    Some(common)
}

/// Shorten hostname by removing the common suffix and replacing with an asterisk
///
/// # Arguments
/// * `hostname` - The original hostname
/// * `common_suffix` - Optional common suffix to remove
///
/// # Returns
/// * `String` - Shortened hostname or original if no common suffix
fn shorten_hostname(hostname: &str, common_suffix: &Option<String>) -> String {
    match common_suffix {
        Some(suffix) if hostname.ends_with(suffix) => {
            let short_name = hostname[..hostname.len() - suffix.len()].to_string();
            format!("{}{}", short_name, "*")
        }
        _ => hostname.to_string(),
    }
}

/// Read and parse the SSH known_hosts file to extract server names
///
/// # Returns
/// * `Vec<Host>` - List of hosts found in the known_hosts file
fn read_known_hosts() -> Vec<Host> {
    let mut result: Vec<Host> = Vec::new();

    // Read known_hosts file from the user's home directory
    for line in read_to_string(format!("/home/{}/.ssh/known_hosts", whoami::username()))
        .unwrap()
        .lines()
    {
        let line = line.split(" ").collect::<Vec<&str>>();
        let hostname = line[0];
        result.push(Host {
            name: hostname.to_string(),
            ip: None,
        })
    }
    result
}

/// Expand a numeric range in the format [start:end] to a list of strings
///
/// # Arguments
/// * `start` - Starting number (inclusive)
/// * `end` - Ending number (inclusive)
///
/// # Returns
/// * `Vec<String>` - List of numbers as strings
fn expand_range(start: i32, end: i32) -> Vec<String> {
    (start..=end).map(|i| i.to_string()).collect()
}

/// Expand a comma-separated list in the format {item1,item2,item3} to a list of strings
///
/// # Arguments
/// * `list` - Comma-separated string to expand
///
/// # Returns
/// * `Vec<String>` - List of expanded items
fn expand_list(list: &str) -> Vec<String> {
    list.split(',').map(|s| s.to_string()).collect()
}

/// Expand a server pattern string with range and list notation into individual hostnames
///
/// Supports two expansion types:
/// - Range expansion: server-[1:5] → server-1, server-2, server-3, server-4, server-5
/// - List expansion: server-{prod,dev} → server-prod, server-dev
///
/// # Arguments
/// * `s` - Pattern string to expand
///
/// # Returns
/// * `Vec<Host>` - List of expanded Host objects
fn expand_string(s: &str) -> Vec<Host> {
    let mut hosts: Vec<Host> = Vec::new();
    let mut result = vec![s.to_string()];

    // First expand all range expressions [start:end]
    while let Some(r) = result.iter().find(|s| s.contains('[')) {
        let r = r.clone();
        let start = r.find('[').unwrap();
        let end = match r[start..].find(']') {
            None => {
                error!("Error parsing host expression. Wrong range expansion '[a:b]'");
                process::exit(1);
            }
            Some(s) => s + start,
        };
        let colon = match r[start..end].find(':') {
            None => {
                error!("Error parsing host expression. Missing colon in range expansion '[a:b]'");
                process::exit(1);
            }
            Some(c) => c + start,
        };
        let low = r[start + 1..colon].parse::<i32>().unwrap();
        let high = r[colon + 1..end].parse::<i32>().unwrap();
        result.retain(|s| s != &r);
        for val in expand_range(low, high) {
            let new_str = format!("{}{}{}", &r[..start], val, &r[end + 1..]);
            result.push(new_str);
        }
    }

    // Then expand all list expressions {item1,item2}
    while let Some(r) = result.iter().find(|s| s.contains('{')) {
        let r = r.clone();
        let start = r.find('{').unwrap();
        let end = match r.find('}') {
            None => {
                error!("Error parsing host expression. Wrong range expansion '{{one,two}}'");
                process::exit(1);
            }
            Some(s) => s,
        };
        let list = &r[start + 1..end];
        result.retain(|s| s != &r);
        for val in expand_list(list) {
            let new_str = format!("{}{}{}", &r[..start], val, &r[end + 1..]);
            result.push(new_str);
        }
    }

    // Convert all expanded strings to Host objects
    for hostname in result {
        hosts.push(Host {
            name: hostname.to_string(),
            ip: None,
        })
    }
    hosts
}

/// Execute a command on a single host using the system SSH client
///
/// This function runs an SSH command using the system's SSH client,
/// capturing and displaying output in real-time with proper formatting.
///
/// # Arguments
/// * `hostname` - Target server hostname
/// * `username` - SSH username
/// * `command` - Command to execute
/// * `common_suffix` - Optional common suffix for hostname display formatting
/// * `code_only` - Whether to display only exit codes
///
/// # Returns
/// * `Result<i32, String>` - Exit code on success or error message
fn execute_ssh_command(
    hostname: &str,
    username: &str,
    command: &str,
    common_suffix: &Option<String>,
    code_only: bool,
) -> Result<i32, String> {
    let display_name = shorten_hostname(hostname, common_suffix);

    // Build the SSH command with appropriate options
    let mut ssh_cmd = Command::new("ssh");
    ssh_cmd
        .arg("-o")
        .arg("StrictHostKeyChecking=no")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg(format!("{}@{}", username, hostname))
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Execute the command
    let mut child = match ssh_cmd.spawn() {
        Ok(child) => child,
        Err(e) => return Err(format!("Failed to start SSH process: {}", e)),
    };

    // Function to handle output lines with proper block management
    let handle_output = |line: String, display_name: &str, code_only: bool| {
        if !code_only {
            let mut current_block = CURRENT_BLOCK.lock().unwrap();

            // Check if we need to close the previous block and open a new one
            match current_block.as_ref() {
                Some(open_host) if open_host != display_name => {
                    // Close the previous block
                    println!("└ {} ┘", open_host.yellow());
                    // Open new block
                    println!("┌ {} ┐", display_name.yellow());
                    *current_block = Some(display_name.to_string());
                }
                None => {
                    // Open new block
                    println!("┌ {} ┐", display_name.yellow());
                    *current_block = Some(display_name.to_string());
                }
                Some(_) => {
                    // Same host, continue with current block
                }
            }

            // Print the log line
            println!("│ {} │ {}", display_name.yellow(), line);
        }
    };

    // Capture and display stdout in real-time using a dedicated thread
    let stdout = child.stdout.take().unwrap();
    let display_name_stdout = display_name.clone();
    let code_only_stdout = code_only;
    let stdout_thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    handle_output(line, &display_name_stdout, code_only_stdout);
                }
                Err(_) => break,
            }
        }
    });

    // Capture and display stderr in real-time using a dedicated thread
    let stderr = child.stderr.take().unwrap();
    let display_name_stderr = display_name.clone();
    let code_only_stderr = code_only;
    let stderr_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    handle_output(line, &display_name_stderr, code_only_stderr);
                }
                Err(_) => break,
            }
        }
    });

    // Wait for command to complete
    let status = match child.wait() {
        Ok(status) => status,
        Err(e) => return Err(format!("Failed to wait for SSH process: {}", e)),
    };

    // Wait for stdout and stderr threads to complete
    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();

    // Close the block if this host was the last one to output
    if !code_only {
        let mut current_block = CURRENT_BLOCK.lock().unwrap();
        if let Some(open_host) = current_block.as_ref() {
            if open_host == &display_name {
                println!("└ {} ┘", display_name.yellow());
                *current_block = None;
            }
        }
    }

    // Format exit code with color (green for success, red for failure)
    let exit_code = status.code().unwrap_or(-1);
    let code_string = if exit_code == 0 {
        format!("{}", exit_code.to_string().green())
    } else {
        format!("{}", exit_code.to_string().red())
    };

    // For code-only mode, just show hostname and exit code
    if code_only {
        println!("{}: [{}]", display_name.yellow(), code_string);
    }

    Ok(exit_code)
}

/// Main entry point for the application
fn main() {
    // Initialize logging with minimal formatting (no timestamp, no target)
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .format_target(false)
        .init();

    // Parse command-line arguments
    let args = Args::parse();

    // Build the list of target hosts based on user selection method
    let hosts = if args.known_hosts {
        // Use regex pattern matching against known_hosts file
        info!("Using ~/.ssh/known_hosts to build server list.");
        let known_hosts = read_known_hosts();
        let mut all_hosts = Vec::new();
        for expression in args.expression.iter() {
            let re = match Regex::new(expression) {
                Ok(result) => result,
                Err(e) => {
                    error!("Error parsing regex. {}", e);
                    process::exit(1);
                }
            };
            let matched: Vec<Host> = known_hosts
                .clone()
                .into_iter()
                .filter(|r| re.is_match(&r.name.clone()))
                .collect();
            all_hosts.extend(matched);
        }
        all_hosts
    } else {
        // Use pattern expansion syntax (ranges and lists)
        info!("Using string expansion to build server list.");
        let mut all_hosts = Vec::new();
        for expression in args.expression.iter() {
            all_hosts.extend(expand_string(expression));
        }
        all_hosts
    };

    // Remove duplicate hosts while preserving original order
    let matched_hosts: Vec<_> = hosts.into_iter().unique().collect();

    // Log parallelism setting if not using the default
    if args.parallel != 100 {
        warn!("Parallelism: {} thread{}", &args.parallel, {
            if args.parallel != 1 {
                "s."
            } else {
                "."
            }
        });
    }

    // Store hosts with their original indices to preserve ordering
    let mut host_with_indices: Vec<(Host, usize)> = Vec::new();
    for (idx, host) in matched_hosts.iter().enumerate() {
        host_with_indices.push((host.clone(), idx));
    }

    info!("Matched hosts:");

    // Perform DNS resolution for all hosts in parallel
    // Results are stored with original indices to maintain order
    let resolved_ips_with_indices = Arc::new(Mutex::new(Vec::<(String, IpAddr, usize)>::new()));

    host_with_indices
        .par_iter()
        .for_each(|(host, idx)| match lookup_host(&host.name) {
            Ok(ips) if !ips.is_empty() => {
                let ip = ips[0];
                let mut results = resolved_ips_with_indices.lock().unwrap();
                results.push((host.name.clone(), ip, *idx));
            }
            Ok(_) => {
                let mut results = resolved_ips_with_indices.lock().unwrap();
                results.push((
                    host.name.clone(),
                    IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
                    *idx,
                ));
            }
            Err(_) => {
                let mut results = resolved_ips_with_indices.lock().unwrap();
                results.push((
                    host.name.clone(),
                    IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
                    *idx,
                ));
            }
        });

    // Sort hosts by original index to maintain consistent display order
    let mut resolved_hosts = resolved_ips_with_indices.lock().unwrap().clone();
    resolved_hosts.sort_by_key(|(_, _, idx)| *idx);

    // Display all matched hosts with their resolved IPs
    for (hostname, ip, _) in &resolved_hosts {
        if ip.is_unspecified() {
            error!("DNS resolve failed: {}", hostname.red());
        } else {
            info!("{} [{}]", hostname, ip);
        }
    }

    // Filter out hosts that couldn't be resolved
    let valid_hosts: Vec<(String, IpAddr, usize)> = resolved_hosts
        .into_iter()
        .filter(|(_, ip, _)| !ip.is_unspecified())
        .collect();

    // Exit if no valid hosts remain
    if valid_hosts.is_empty() {
        error!("No valid hosts to connect to");
        process::exit(1);
    }

    // Find common domain suffix to optimize display
    let hostnames: Vec<String> = valid_hosts
        .iter()
        .map(|(hostname, _, _)| hostname.clone())
        .collect();
    let common_suffix = find_common_suffix(&hostnames);

    // Inform user about display optimization if common suffix found
    if let Some(suffix) = &common_suffix {
        info!(
            "Common domain suffix found: '{}' (will be displayed as '*')",
            suffix
        );
    }

    // Ask for confirmation before proceeding (unless --noconfirm is specified)
    if !args.noconfirm
        && match Question::new(&*format!(
            "Continue on following {} servers?",
            &valid_hosts.len()
        ))
        .confirm()
        {
            Answer::YES => true,
            Answer::NO => {
                warn!("Stopped");
                process::exit(0);
            }
            _ => unreachable!(),
        }
    {
        info!("Run command on {} servers.", &valid_hosts.len());
    }

    // Execute commands using system SSH client
    let batch_size = args.parallel as usize;
    let mut processed = 0;

    while processed < valid_hosts.len() {
        let end = std::cmp::min(processed + batch_size, valid_hosts.len());
        let batch = &valid_hosts[processed..end];

        // Create a thread for each host in the current batch
        let mut handles = Vec::new();

        for (hostname, _, _) in batch {
            let hostname = hostname.clone();
            let username = args.username.clone();
            let command = args.command.clone();
            let common_suffix_clone = common_suffix.clone();
            let code_only = args.code;

            // Execute SSH command in a separate thread
            let handle = thread::spawn(move || {
                match execute_ssh_command(
                    &hostname,
                    &username,
                    &command,
                    &common_suffix_clone,
                    code_only,
                ) {
                    Ok(_) => (),
                    Err(e) => error!("Error executing command on {}: {}", hostname, e),
                }
            });

            handles.push(handle);
        }

        // Wait for all threads in this batch to complete
        for handle in handles {
            handle.join().unwrap();
        }

        processed = end;
    }
}

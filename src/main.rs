#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::fs::read_to_string;
use std::hash::Hash;
use std::net::IpAddr;
use std::process;
use std::sync::{Arc, Mutex};

use clap::Parser;
use colored::*;
use dns_lookup::lookup_host;
use env_logger::Env;
use itertools::Itertools;
use log::{error, info, warn};
use massh::{MasshClient, MasshConfig, MasshHostConfig, SshAuth};
use question::{Answer, Question};
use rayon::prelude::*;
use regex::Regex;

// Define args
#[derive(Parser, Debug)]
#[command(author = "AB ab@hexor.ru", version, about = "Parallel SSH executor in Rust", long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = whoami::username())]
    username: String,

    #[arg(
        short,
        long,
        help = "Use known_hosts to build servers list instead of string expansion."
    )]
    known_hosts: bool,

    #[arg(
        short,
        long,
        num_args = 1..,
        help = "Expression to build server list. List and range expansion are supported. Example: 'web-[1:12]-io-{prod,dev}'"
    )]
    expression: Vec<String>,

    #[arg(short, long, help = "Command to execute on servers")]
    command: String,

    #[arg(long, default_value_t = false, help = "Show exit code ONLY")]
    code: bool,

    #[arg(
        short = 'f',
        long,
        default_value_t = false,
        help = "Don't ask for confirmation"
    )]
    noconfirm: bool,

    #[arg(short, long, default_value_t = 100)]
    parallel: i32,
}

// Represent line from known_hosts file
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Host {
    name: String,
    ip: Option<IpAddr>,
}

// Read known_hosts file
fn read_known_hosts() -> Vec<Host> {
    let mut result: Vec<Host> = Vec::new();

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

fn expand_range(start: i32, end: i32) -> Vec<String> {
    (start..=end).map(|i| i.to_string()).collect()
}

fn expand_list(list: &str) -> Vec<String> {
    list.split(',').map(|s| s.to_string()).collect()
}

fn expand_string(s: &str) -> Vec<Host> {
    let mut hosts: Vec<Host> = Vec::new();
    let mut result = vec![s.to_string()];

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

    for hostname in result {
        hosts.push(Host {
            name: hostname.to_string(),
            ip: None,
        })
    }
    hosts
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .format_target(false)
        .init();
    let args = Args::parse();

    let hosts = if args.known_hosts {
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
        info!("Using string expansion to build server list.");
        let mut all_hosts = Vec::new();
        for expression in args.expression.iter() {
            all_hosts.extend(expand_string(expression));
        }
        all_hosts
    };

    // Dedup hosts from known_hosts file but preserve original order
    let matched_hosts: Vec<_> = hosts.into_iter().unique().collect();

    // Build MasshHostConfig hostnames list
    if args.parallel != 100 {
        warn!("Parallelism: {} thread{}", &args.parallel, {
            if args.parallel != 1 {
                "s."
            } else {
                "."
            }
        });
    }

    // Store hosts with their indices to preserve order
    let mut host_with_indices: Vec<(Host, usize)> = Vec::new();
    for (idx, host) in matched_hosts.iter().enumerate() {
        host_with_indices.push((host.clone(), idx));
    }

    info!("Matched hosts:");

    // Do DNS resolution in parallel but store results for ordered display later
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

    // Sort by original index to ensure hosts are displayed in order
    let mut resolved_hosts = resolved_ips_with_indices.lock().unwrap().clone();
    resolved_hosts.sort_by_key(|(_, _, idx)| *idx);

    // Now print the hosts in the correct order
    for (hostname, ip, _) in &resolved_hosts {
        if ip.is_unspecified() {
            error!("DNS resolve failed: {}", hostname.red());
        } else {
            info!("{} [{}]", hostname, ip);
        }
    }

    // Create massh_hosts in the correct order
    let mut hosts_and_ips: HashMap<IpAddr, (String, usize)> = HashMap::new();
    let mut massh_hosts: Vec<MasshHostConfig> = Vec::new();

    for (hostname, ip, idx) in resolved_hosts {
        // Skip hosts that couldn't be resolved
        if !ip.is_unspecified() {
            hosts_and_ips.insert(ip, (hostname.clone(), idx));
            massh_hosts.push(MasshHostConfig {
                addr: ip,
                auth: None,
                port: None,
                user: None,
            });
        }
    }

    // Process hosts in batches to maintain order
    let batch_size = args.parallel as usize;

    // Ask for confirmation
    if !massh_hosts.is_empty()
        && (args.noconfirm
            || match Question::new(&*format!(
                "Continue on following {} servers?",
                &massh_hosts.len()
            ))
            .confirm()
            {
                Answer::YES => true,
                Answer::NO => false,
                _ => unreachable!(),
            })
    {
        info!("Run command on {} servers.", &massh_hosts.len());

        let mut processed = 0;

        while processed < massh_hosts.len() {
            let end = std::cmp::min(processed + batch_size, massh_hosts.len());

            // Create a new config and vector for this batch
            let mut batch_hosts = Vec::new();
            for host in &massh_hosts[processed..end] {
                batch_hosts.push(MasshHostConfig {
                    addr: host.addr,
                    auth: None,
                    port: None,
                    user: None,
                });
            }

            // Create a new MasshClient for this batch
            let batch_config = MasshConfig {
                default_auth: SshAuth::Agent,
                default_port: 22,
                default_user: args.username.clone(),
                threads: batch_hosts.len() as u64,
                timeout: 0,
                hosts: batch_hosts,
            };

            let batch_massh = MasshClient::from(&batch_config);

            // Run commands on this batch
            let rx = batch_massh.execute(args.command.clone());

            // Collect all results from this batch before moving to the next
            let mut batch_results = Vec::new();

            while let Ok((host, result)) = rx.recv() {
                let ip: String = host.split('@').collect::<Vec<_>>()[1]
                    .split(':')
                    .collect::<Vec<_>>()[0]
                    .to_string();
                let ip = ip.parse::<IpAddr>().unwrap();

                if let Some((hostname, idx)) = hosts_and_ips.get(&ip) {
                    batch_results.push((hostname.clone(), ip, result, *idx));
                } else {
                    error!("Unexpected IP address in result: {}", ip);
                }
            }

            // Sort the batch results by index to ensure they're displayed in order
            batch_results.sort_by_key(|(_, _, _, idx)| *idx);

            // Display the results
            for (hostname, _ip, result, _) in batch_results {
                println!("\n{}", hostname.yellow().bold().to_string());

                let output = match result {
                    Ok(output) => output,
                    Err(e) => {
                        error!("Can't access server: {}", e);
                        continue;
                    }
                };

                let code_string = if output.exit_status == 0 {
                    format!("{}", output.exit_status.to_string().green())
                } else {
                    format!("{}", output.exit_status.to_string().red())
                };

                println!(
                    "{}",
                    format!(
                        "Exit code [{}] / stdout {} bytes / stderr {} bytes",
                        code_string,
                        output.stdout.len(),
                        output.stderr.len()
                    )
                    .bold()
                );

                if !args.code {
                    match String::from_utf8(output.stdout) {
                        Ok(stdout) => match stdout.as_str() {
                            "" => {}
                            _ => {
                                let prefix = if output.exit_status != 0 {
                                    format!("{}", "│".cyan())
                                } else {
                                    format!("{}", "│".green())
                                };
                                for line in stdout.lines() {
                                    println!("{} {}", prefix, line);
                                }
                            }
                        },
                        Err(_) => {}
                    }
                    match String::from_utf8(output.stderr) {
                        Ok(stderr) => match stderr.as_str() {
                            "" => {}
                            _ => {
                                for line in stderr.lines() {
                                    println!("{} {}", "║".red(), line);
                                }
                            }
                        },
                        Err(_) => {}
                    }
                }
            }

            processed = end;
        }
    } else {
        warn!("Stopped");
    }
}

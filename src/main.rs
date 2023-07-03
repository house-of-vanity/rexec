#[macro_use]
extern crate log;

use std::collections::HashMap;
use std::fs::read_to_string;
use std::hash::Hash;
use std::net::IpAddr;
use std::process;

use clap::Parser;
use colored::*;
use dialoguer::Confirm;
use dns_lookup::lookup_host;
use env_logger::Env;
use itertools::Itertools;
use log::{error, info};
use massh::{MasshClient, MasshConfig, MasshHostConfig, SshAuth};
use regex::Regex;

// Define args
#[derive(Parser, Debug)]
#[command(author = "AB ab@hexor.ru", version, about = "Parallel SSH executor in Rust", long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = whoami::username())]
    username: String,

    #[arg(short, long, help = "Use known_hosts to build servers list")]
    known_hosts: bool,

    #[arg(
        short,
        long,
        help = "Expression to build server list. List and range expansion available. Example: 'web-[1:12]-io-{prod,dev}'"
    )]
    expression: String,

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

fn expand_string(string: &str) -> Vec<Host> {
    let mut hosts: Vec<Host> = Vec::new();

    // Bracket expansion
    let mut parts: Vec<&str> = string.split('[').collect();
    let mut expanded_brackets: Vec<String> = vec![parts[0].to_string()];

    for part in parts[1..].iter() {
        let mut split = part.splitn(2, ']');
        let range_string = split.next().unwrap();
        let suffix = split.next().unwrap();

        let mut range_parts = range_string.split(':');
        let range_start: usize = range_parts.next().unwrap().parse().unwrap();
        let range_end: usize = range_parts.next().unwrap_or(range_string).parse().unwrap();

        expanded_brackets = expanded_brackets
            .into_iter()
            .flat_map(|s| (range_start..=range_end).map(move |i| format!("{}{}{}", s, i, suffix)))
            .collect();
    }


    // Brace expansion
    let mut result: Vec<String> = Vec::new();


        let mut expanded_strings: Vec<String> = vec![String::from(string)];

        while let Some(open_brace_index) = expanded_strings
            .iter()
            .find(|s| s.contains('{'))
            .and_then(|s| s.find('{'))
        {
            let mut new_expanded_strings: Vec<String> = Vec::new();

            for exp_string in expanded_strings {
                if let Some(close_brace_index) = exp_string[open_brace_index..].find('}') {
                    let prefix = &exp_string[..open_brace_index];
                    let list = &exp_string[open_brace_index + 1..open_brace_index + close_brace_index];
                    let postfix = &exp_string[open_brace_index + close_brace_index + 1..];

                    let items: Vec<&str> = list.split(',').collect();

                    for item in items {
                        let new_exp_string = format!("{}{}{}", prefix, item, postfix);
                        new_expanded_strings.push(new_exp_string);
                    }
                }
            }

            expanded_strings = new_expanded_strings;
        }

        result.extend(expanded_strings);
        result.extend(expanded_brackets);



    for hostname in result {
        hosts.push(Host {
            name: hostname.to_string(),
            ip: None,
        })
    }
    hosts
}

fn _expand_strings(string: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();

    let mut expanded_strings: Vec<String> = vec![String::from(string)];

    while let Some(open_brace_index) = expanded_strings
        .iter()
        .find(|s| s.contains('{'))
        .and_then(|s| s.find('{'))
    {
        let mut new_expanded_strings: Vec<String> = Vec::new();

        for exp_string in expanded_strings {
            if let Some(close_brace_index) = exp_string[open_brace_index..].find('}') {
                let prefix = &exp_string[..open_brace_index];
                let list = &exp_string[open_brace_index + 1..open_brace_index + close_brace_index];
                let postfix = &exp_string[open_brace_index + close_brace_index + 1..];

                let items: Vec<&str> = list.split(',').collect();

                for item in items {
                    let new_exp_string = format!("{}{}{}", prefix, item, postfix);
                    new_expanded_strings.push(new_exp_string);
                }
            }
        }

        expanded_strings = new_expanded_strings;
    }

    result.extend(expanded_strings);

    result
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
        // Build regex
        let re = match Regex::new(&args.expression) {
            Ok(result) => result,
            Err(e) => {
                error!("Error parsing regex. {}", e);
                process::exit(1);
            }
        };
        // match hostnames from known_hosts to regex
        known_hosts
            .into_iter()
            .filter(|r| re.is_match(&r.name.clone()))
            .collect()
    } else {
        info!("Using string expansion to build server list.");
        expand_string(&args.expression)
    };

    // Dedup hosts from known_hosts file
    let matched_hosts: Vec<_> = hosts.into_iter().unique().collect();

    // Build MasshHostConfig hostnames list
    let mut massh_hosts: Vec<MasshHostConfig> = vec![];
    let mut hosts_and_ips: HashMap<IpAddr, String> = HashMap::new();
    info!("Matched hosts:");
    for host in matched_hosts.iter() {
        let ip = match lookup_host(&host.name) {
            Ok(ip) => ip[0],
            Err(_) => {
                error!("{} couldn't be resolved.", &host.name.red());
                continue;
            }
        };
        info!("{} [{}]", &host.name, ip);
        hosts_and_ips.insert(ip, host.name.clone());
        massh_hosts.push(MasshHostConfig {
            addr: ip,
            auth: None,
            port: None,
            user: None,
        })
    }
    // Build MasshConfig using massh_hosts vector
    let config = MasshConfig {
        default_auth: SshAuth::Agent,
        default_port: 22,
        default_user: args.username,
        threads: args.parallel as u64,
        timeout: 0,
        hosts: massh_hosts,
    };
    let massh = MasshClient::from(&config);

    // Ask for confirmation
    if args.noconfirm == true
        || Confirm::new()
            .with_prompt(format!(
                "Continue on following {} servers?",
                &config.hosts.len()
            ))
            .interact()
            .unwrap()
    {
        info!("\n");
        info!("Run command on {} servers.", &config.hosts.len());
        info!("\n");

        // Run a command on all the configured hosts.
        // Receive the result of the command for each host and print its output.
        let rx = massh.execute(args.command);

        while let Ok((host, result)) = rx.recv() {
            let ip: String = host.split('@').collect::<Vec<_>>()[1]
                .split(':')
                .collect::<Vec<_>>()[0]
                .to_string();
            let ip = ip.parse::<IpAddr>().unwrap();
            info!(
                "{}",
                hosts_and_ips
                    .get(&ip)
                    .unwrap_or(&"Couldn't parse IP".to_string())
                    .to_string()
                    .yellow()
                    .bold()
                    .to_string()
            );
            let output = match result {
                Ok(output) => output,
                Err(e) => {
                    error!("Can't access server: {}", e);
                    continue;
                }
            };
            if output.exit_status == 0 {
                println!("Code {}", output.exit_status);
            } else {
                error!("Code {}", output.exit_status);
            };
            if !args.code {
                println!("STDOUT:\n{}", String::from_utf8(output.stdout).unwrap());
                println!("STDERR:\n{}", String::from_utf8(output.stderr).unwrap());
            }
        }
    } else {
        warn!("Stopped");
    }
}

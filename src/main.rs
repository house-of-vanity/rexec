use colored::*;
use dialoguer::Confirm;
use dns_lookup::lookup_host;
use env_logger::Env;
use log::{error, info};
use massh::{MasshClient, MasshConfig, MasshHostConfig, SshAuth};
use regex::Regex;
use std::fs::read_to_string;
use std::net::IpAddr;

#[macro_use]
extern crate log;

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long, default_value_t = whoami::username())]
    username: String,

    #[arg(short, long)]
    pattern: String,

    #[arg(short, long)]
    command: String,
}

#[derive(Debug, Default)]
struct KnownHost {
    name: String,
    ip: Option<IpAddr>,
}

impl KnownHost {
    fn new(name: String) -> KnownHost {
        KnownHost { name, ip: None }
    }
}

fn read_known_hosts() -> Vec<KnownHost> {
    let mut result: Vec<KnownHost> = Vec::new();

    for line in read_to_string(format!("/home/{}/.ssh/known_hosts", whoami::username()))
        .unwrap()
        .lines()
    {
        let line = line.split(" ").collect::<Vec<&str>>();
        let hostname = line[0];
        // let ip = match lookup_host(hostname) {
        //     Ok(ip) => ip[0],
        //     Err(_) => continue
        // };
        result.push(KnownHost {
            name: hostname.to_string(),
            ip: None,
        })
    }
    result
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .format_target(false)
        .init();
    let args = Args::parse();

    let known_hosts = read_known_hosts();
    let re = Regex::new(&args.pattern).unwrap();
    let mut matched_hosts: Vec<KnownHost> = known_hosts
        .into_iter()
        .filter(|r| re.is_match(&r.name.clone()))
        //.filter(|a| a.access.iter().any(|b| b.users.contains(&user)))
        .collect();

    let mut massh_hosts: Vec<MasshHostConfig> = vec![];
    info!("Matched hosts:");
    for host in matched_hosts.iter() {
        let ip = match lookup_host(&host.name) {
            Ok(ip) => ip[0],
            Err(_) => {
                error!("{} couldn't ve resolved.", &host.name.red());
                continue;
            }
        };
        info!("{} [{}]", &host.name, ip);
        massh_hosts.push(MasshHostConfig {
            addr: ip,
            auth: None,
            port: None,
            user: None,
        })
    }

    let config = MasshConfig {
        default_auth: SshAuth::Agent,
        default_port: 22,
        //default_user: whoami::username(),
        default_user: "abogomyakov".to_string(),
        threads: 10,
        timeout: 0,
        hosts: massh_hosts,
    };
    let massh = MasshClient::from(&config);

    // Receive the result of the command for each host and print its output.

    if Confirm::new()
        .with_prompt(format!(
            "Continue on following {} servers?",
            &config.hosts.len()
        ))
        .interact()
        .unwrap()
    {
        info!("\n");
        info!("Run command on {} servers.", &config.hosts.len());
        info!("\n"); // Run a command on all the configured hosts.
        let rx = massh.execute(args.command);

        while let Ok((host, result)) = rx.recv() {
            info!("{}", host.yellow().bold());
            let output = result.unwrap();
            if output.exit_status == 0 {
                info!("Code {}", output.exit_status.to_string().green());
            } else {
                info!("Code {}", output.exit_status.to_string().red());
            };

            info!("STDOUT:\n{}", String::from_utf8(output.stdout).unwrap());
            info!("STDERR:\n{}", String::from_utf8(output.stderr).unwrap());
        }
    } else {
        warn!("Stopped");
    }
}

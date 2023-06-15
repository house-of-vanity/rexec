use std::net::{IpAddr, Ipv4Addr};
use massh::{MasshClient, MasshConfig, MasshHostConfig, SshAuth};
use dns_lookup::lookup_host;

fn main() {
    // Construct a new `MasshClient` from a YAML configuration file.
    //let yaml = std::fs::read_to_string("massh.yaml").unwrap();
    //let config = MasshConfig::from_yaml(&yaml).unwrap();
    //println!("{:?} {:?}", lookup_host("fast.hexor.ru"), lookup_host("vpn.hexor.ru"));
    let config = MasshConfig {
        default_auth: SshAuth::Agent,
        default_port: 22,
        default_user: "abogomyakov".to_string(),
        threads: 10,
        timeout: 0,
        hosts: vec![
            MasshHostConfig {
                addr: lookup_host("admin.zth-dev.logmatching.iponweb.net").unwrap()[0],
                //addr: IpAddr::V4(Ipv4Addr::new(35,211,176,68)),
                auth: None,
                port: None,
                user: None,
            },
            MasshHostConfig {
                addr: lookup_host("admin.cbr-prod.logmatching.iponweb.net").unwrap()[0],
                auth: None,
                port: None,
                user: None,
            }
        ],
    };
    let massh = MasshClient::from(&config);


    // Run a command on all the configured hosts.
    let rx = massh.execute("uptime");

    // Receive the result of the command for each host and print its output.
    while let Ok((host, result)) = rx.recv() {
        let output = result.unwrap();
        println!("host: {}", host);
        println!("status: {}", output.exit_status);
        println!("stdout: {}", String::from_utf8(output.stdout).unwrap());
        println!("stderr: {}", String::from_utf8(output.stderr).unwrap());
    }
}

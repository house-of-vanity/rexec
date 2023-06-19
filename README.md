# rexec
Parallel SSH executor in Rust. Read hosts from `known_hosts`
[![Rust-static-build](https://github.com/house-of-vanity/rexec/actions/workflows/release.yml/badge.svg)](https://github.com/house-of-vanity/rexec/actions/workflows/release.yml)
```
[ab@test debug]$ ./rexec -k admin-gce-sc.* --help
Usage: rexec [OPTIONS] --kh <KNOWN_HOSTS> --command <COMMAND>

Options:
  -u, --username <USERNAME>  [default: ab]
  -k, --kh <KNOWN_HOSTS>     Use known_hosts to build servers list
  -c, --command <COMMAND>    Command to execute on servers
      --code                 Show exit code ONLY
      --noconfirm            Don't ask for confirmation
  -p, --parallel <PARALLEL>  [default: 100]
  -h, --help                 Print help
  -V, --version              Print version
  
  
[ab@test debug]$ ./rexec -k admin-gce-sc.* -c uptime -u ab 
[INFO ] Matched hosts:
[INFO ] admin-gce-sc-1.lca-prod.** [35.211.27.195]
[INFO ] admin-gce-sc-1.mmk-prod.** [35.211.79.202]
[ERROR] admin-gce-sc-1.led-prod.** couldn't ve resolved.
[INFO ] admin-gce-sc-1.msq-dev.** [35.211.0.24]
[ERROR] admin-gce-sc-1.hui-dev.** couldn't ve resolved.
Continue on following 3 servers? yes
[INFO ] 
    
[INFO ] Run command on 3 servers.
[INFO ] 
    
[INFO ] admin-gce-sc-1.mmk-prod.**
[INFO ] Code 0
[INFO ] STDOUT:
     10:20:02 up 284 days, 23:04,  0 users,  load average: 0.00, 0.08, 0.16
    
[INFO ] STDERR:
    
[INFO ] admin-gce-sc-1.msq-dev.**
[INFO ] Code 0
[INFO ] STDOUT:
     10:20:03 up 186 days, 23:52,  1 user,  load average: 0.12, 0.12, 0.16
    
[INFO ] STDERR:
    
[INFO ] admin-gce-sc-1.lca-prod.**
[INFO ] Code 0
[INFO ] STDOUT:
     10:20:03 up 292 days,  8:35,  0 users,  load average: 0.57, 0.49, 0.47
    
[INFO ] STDERR:
```

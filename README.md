# rexec
**Parallel SSH executor** in Rust with static binary. It can read servers from `~/.ssh/known_hosts`

or even expand servers from bash expanshion line `cassandra-[1:5].{prod,dev}.example.com`

[![Rust-static-build](https://github.com/house-of-vanity/rexec/actions/workflows/release.yml/badge.svg)](https://github.com/house-of-vanity/rexec/actions/workflows/release.yml)
---

## Usage
```shell                                                    
            _|_|_|_|   _|      _|   _|_|_|_|     _|_|_|  
 _|  _|_|   _|           _|  _|     _|         _|        
 _|_|       _|_|_|         _|       _|_|_|     _|        
 _|         _|           _|  _|     _|         _|        
 _|         _|_|_|_|   _|      _|   _|_|_|_|     _|_|_|  
                                                        

Parallel SSH executor in Rust

Usage: rexec [OPTIONS] --expression <EXPRESSION> --command <COMMAND>

Options:
  -u, --username <USERNAME>      [default: ab]
  -k, --known-hosts              Use known_hosts to build servers list
  -e, --expression <EXPRESSION>  Expression to build server list
  -c, --command <COMMAND>        Command to execute on servers
      --code                     Show exit code ONLY
  -f, --noconfirm                Don't ask for confirmation
  -p, --parallel <PARALLEL>      [default: 100]
  -h, --help                     Print help
  -V, --version                  Print version
```
---

![image](https://github.com/house-of-vanity/rexec/assets/4666566/4c52915d-2bc1-46b9-9833-b0d7c0527f2d)


## Examples
```shell
$ rexec -f \
    -e 'cassandra-gce-or-[1:2]' \
    -u ab \
    -c 'uname -r; date'
[INFO ] Using string expansion to build server list.
[INFO ] Matched hosts:
[INFO ] cassandra-gce-or-1.prod.example.com [2.22.123.79]
[INFO ] cassandra-gce-or-2.prod.example.com [2.22.123.158]
Continue on following 2 servers? y
[INFO ] Run command on 2 servers.

cassandra-gce-or-1.prod.example.com
Exit code [0] / stdout 45 bytes / stderr 0 bytes
STDOUT
║ 5.15.0-1040-gcp
║ Thu Sep  7 13:44:40 UTC 2023

cassandra-gce-or-2.prod.example.com
Exit code [0] / stdout 45 bytes / stderr 0 bytes
STDOUT
║ 5.15.0-1040-gcp
║ Thu Sep  7 13:44:40 UTC 2023
```

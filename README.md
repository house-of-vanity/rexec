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

## Examples
```shell
[ab@test]$ rexec -e 'cassandra-gce-sc-[1:3].bbs-prod.*' -u ab -c 'df -h /srv/cassandra'
[INFO ] Using string expansion to build server list.
[INFO ] Matched hosts:
[INFO ] cassandra-gce-sc-1.bbs-prod.* [35.221.18.35]
[INFO ] cassandra-gce-sc-2.bbs-prod.* [35.212.13.174]
[INFO ] cassandra-gce-sc-3.bbs-prod.* [35.211.213.123]
Continue on following 3 servers? y
[INFO ] 
    
[INFO ] Run command on 3 servers.
[INFO ] 
    
[INFO ] cassandra-gce-sc-2.bbs-prod.*
Code 0
STDOUT:
Filesystem                     Size  Used Avail Use% Mounted on
/dev/mapper/storage-cassandra  1.0T  613G  411G  60% /srv/cassandra

[INFO ] cassandra-gce-sc-1.bbs-prod.*
Code 0
STDOUT:
Filesystem                     Size  Used Avail Use% Mounted on
/dev/mapper/storage-cassandra  1.0T  594G  430G  59% /srv/cassandra

[INFO ] cassandra-gce-sc-3.bbs-prod.*
Code 0
STDOUT:
Filesystem                     Size  Used Avail Use% Mounted on
/dev/mapper/storage-cassandra  1.0T  523G  502G  52% /srv/cassandra
```
---

```shell
[ab@test]$ ./rexec -u ab -k -c uptime -e admin.* -f
[INFO ] Matched hosts:
[INFO ] admin-gce-sc-1.lca-prod.** [35.211.27.195]
[INFO ] admin-gce-sc-1.mmk-prod.** [35.211.79.202]
[ERROR] admin-gce-sc-1.led-prod.** couldn't be resolved.
[INFO ] admin-gce-sc-1.msq-dev.** [35.211.0.24]
[ERROR] admin-gce-sc-1.hui-dev.** couldn't be resolved.
    
[INFO ] Run command on 3 servers.

[INFO ] admin.gnb-prod.**
Code 0 
STDOUT:                                                    
 23:31:21 up 294 days, 14:14,  0 users,  load average: 0.53, 0.64, 0.52
                                                                                                                       
[INFO ] admin.abe-prod.**
Code 0 
STDOUT:                                                    
 23:31:22 up 154 days,  9:24,  0 users,  load average: 0.31, 0.25, 0.18

[INFO ] admin-gce-be-1.toy-prod.**
Code 0                                                     
STDOUT:
 23:31:22 up 98 days,  6:20,  0 users,  load average: 0.88, 0.74, 0.80
```

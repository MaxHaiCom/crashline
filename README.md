# crashline

一个只有 WAL 的小数据库。页大小 4096，校验和是页尾的 CRC32C。恢复在半页或坏校验处截断，只重放最后一个完整提交。

```sh
cargo test
```

依赖只有 `crc32c` 和 `libc`。Linux 上 `libc::fdatasync` 同步文件内容。子进程通过 `crashline child` 重新 exec，父进程不继承 WAL 的文件描述符。

Bot 约束在 [PLAYBOOK.md](PLAYBOOK.md)。

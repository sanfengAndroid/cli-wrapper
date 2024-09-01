# cli-wrapper

[English](README.md) | [简体中文](README_CN.md)

`cli-wrapper` 如其名是一款由 `Rust` 编写的跨平台的通用命令行包装器, 提供多种功能修改命令行参数,重定向输出,打印日志等等功能, 主要用在软件编译的过程

## 编译

```shell
cargo build --release
```

## 功能

以 `-clw-` 开头的参数作为内部配置参数目前支持以下参数, 当前版本非正式发布版本, 可能代码变动较大, 具体可以查看代码 [main.rs](src/main.rs) 实现.
`cli-wrapper` 支持 `gcc/clang` 编译器支持的 `ResponseFile` 参数, 当 `cli-wrapper` 无法解析 `-clw-` 的配置时则保留在命令行中

| 关键字                             | 描述                                                                                                                                        |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `-clw-just-print`                  | 仅打印最终执行的命令,不执行                                                                                                                 |
| `-clw-before-print`                | 在执行实际命令之前打印最终执行的命令和参数                                                                                                  |
| `-clw-log-file=<日志文件>`         | 以追加的方式将 `cli-wrapper` 内部的日志重定向到文件                                                                                         |
| `-clw-command=<命令>`              | 使用 `命令` 替换当前程序执行, 其它参数不变                                                                                                  |
| `-clw-work-dir=<工作路径>`         | 改变命令执行的工作路径                                                                                                                      |
| `-clw-redirect-stdout=<文件路径>`  | 重定向 `stdout` 到指定文件, 可以同 `stderr` 重定向相同路径                                                                                  |
| `-clw-redirect-stderr=<文件路径>`  | 重定向 `stderr` 到指定文件, 可以同 `stdout` 重定向相同路径                                                                                  |
| `-clw-command=<替换命令>`          | 替换执行的命令                                                                                                                              |
| `-clw-remove=<arg>`                | 删除所有 `<arg>` 命令行参数                                                                                                                 |
| `-clw-replace-<before>=<after>`    | 替换命令行所有 `<before>` 参数为`<after>`                                                                                                   |
| `-clw-static-link-compiler=<arg>`  | 替换链接命令中 `<arg>` 库为静态链接, 它会删除之前所有的 `<arg>` 参数然后再末尾添加 `-Wl,-Bstatic`, `-Wl,<arg>`适用于 `gcc`/`clang`等编译器  |
| `-clw-dynamic-link-compiler=<arg>` | 替换链接命令中 `<arg>` 库为动态链接, 它会删除之前所有的 `<arg>` 参数然后再末尾添加 `-Wl,-Bdynamic`, `-Wl,<arg>`适用于 `gcc`/`clang`等编译器 |
| `-clw-static-link=<arg>`           | 替换链接命令中 `<arg>` 库为静态链接, 它会删除之前所有的 `<arg>` 参数然后再末尾添加 `-Bstatic`, `<arg>`适用于 `ld`/`lld`等链接器             |
| `-clw-dynamic-link=<arg>`          | 替换链接命令中 `<arg>` 库为动态链接, 它会删除之前所有的 `<arg>` 参数然后再末尾添加 `-Bdynamic`, `<arg>`适用于 `ld`/`lld`等链接器            |

## 示例

1. 链接命令中的所有动态链接 `libc.so` 改为静态链接 `libc.a`

   ```shell
   # gcc/clang编译器驱动链接器, 注意 <arg> 为 -lc
   cli-wrapper gcc <原始命令行参数> -clw-static-link-compiler=-lc

   # ld/lld链接器替换
   cli-wrapper ld <原始命令行参数> -clw-static-link=-lc
   ```

2. 删除所有参数 `-lm`
   ```shell
   cli-wrapper ld <原始命令行参数> -clw-remove=-lm
   ```
3. 替换所有参数 `-lm` 为 `-lm2`

   ```shell
   cli-wrapper ld <原始命令行参数> -clw-replace--lm=-lm2
   ```

4. 执行前打印最终执行的命令

   ```shell
   cli-wrapper ld <原始命令行参数> -clw-before-print
   ```

5. 重定向 `stdout` 到文件
   ```shell
   cli-wrapper ld <原始命令行参数> -clw-redirect-stdout=output.txt
   ```

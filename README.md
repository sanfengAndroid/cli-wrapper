# cli-wrapper

[English](README.md) | [简体中文](README_CN.md)

`cli-wrapper` as its name implies, is a cross-platform command line wrapper written in Rust. It provides various functionalities such as modifying command line arguments, redirecting output, printing logs, etc. It is mainly used in the software compilation process.

## Build

```shell
cargo build --release
```

## Features

The parameters starting with `-clw-` are used as internal configuration parameters. Currently, the following parameters are supported. Please note that the current version is not an official release version, so there may be significant code changes. For more details, you can refer to the implementation in [main.rs](src/main.rs).
`cli-wrapper` supports the `ResponseFile` parameters supported by the `gcc/clang` compilers. If `cli-wrapper` cannot parse the `-clw-` configuration, it will be preserved in the command line.

| Keyword                             | Description                                                                                                                                                                                                    |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-clw-just-print`                   | Only print the final executed command without actually executing it                                                                                                                                            |
| `-clw-work-dir=<working directory>` | Change the working directory for command execution                                                                                                                                                             |
| `-clw-redirect-stdout=<file path>`  | Redirect `stdout` to the specified file, can be the same path as `stderr`                                                                                                                                      |
| `-clw-redirect-stderr=<file path>`  | Redirect `stderr` to the specified file, can be the same path as `stdout`                                                                                                                                      |
| `-clw-before-print`                 | Print the final executed command and its arguments before actually executing it                                                                                                                                |
| `-clw-static-link-compiler=<arg>`   | Replace the `<arg>` library in the linking command with static linking. It will remove all previous `<arg>` arguments and append `-Wl,-Bstatic`, `-Wl,<arg>`. Applicable to compilers such as `gcc`/`clang`.   |
| `-clw-dynamic-link-compiler=<arg>`  | Replace the `<arg>` library in the linking command with dynamic linking. It will remove all previous `<arg>` arguments and append `-Wl,-Bdynamic`, `-Wl,<arg>`. Applicable to compilers such as `gcc`/`clang`. |
| `-clw-static-link=<arg>`            | Replace the `<arg>` library in the linking command with static linking. It will remove all previous `<arg>` arguments and append `-Bstatic`, `<arg>`. Applicable to linkers such as `ld`/`lld`.                |
| `-clw-dynamic-link=<arg>`           | Replace the `<arg>` library in the linking command with dynamic linking. It will remove all previous `<arg>` arguments and append `-Bdynamic`, `<arg>`. Applicable to linkers such as `ld`/`lld`.              |
| `-clw-remove=<arg>`                 | Remove all `<arg>` command line arguments                                                                                                                                                                      |
| `-clw-replace-<before>=<after>`     | Replace all `<before>` command line arguments with `<after>`                                                                                                                                                   |

## Examples

1. Replace all dynamic link `libc.so` with static link `libc.a` in the linking command

   ```shell
   # For gcc/clang compiler driving linker, where <arg> is -lc
   cli-wrapper gcc <original arguments> -clw-static-link-compiler=-lc

   # For ld/lld linker replacement
   cli-wrapper ld <original arguments> -clw-static-link=-lc
   ```

2. Remove all arguments `-lm`

   ```shell
   cli-wrapper ld <original arguments> -clw-remove=-lm
   ```

3. Replace all arguments `-lm` with `-lm2`

   ```shell
   cli-wrapper ld <original arguments> -clw-replace--lm=-lm2
   ```

4. Print the final executed command before execution

   ```shell
   cli-wrapper ld <original arguments> -clw-before-print
   ```

5. Redirect `stdout` to a file
   ```shell
   cli-wrapper ld <original arguments> -clw-redirect-stdout=output.txt
   ```

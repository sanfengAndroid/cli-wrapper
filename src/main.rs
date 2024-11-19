use anyhow::Result;
use simplelog::*;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::process::{Command, Stdio};

struct ResponseFile {
    original_path: String,
    new_path: String,
    values: Vec<String>,
    changed: bool,
}

impl ResponseFile {
    fn new(original_path: String, new_path: String) -> ResponseFile {
        ResponseFile {
            original_path: original_path.clone(),
            new_path,
            values: Self::read_response_file(&original_path).unwrap_or(vec![]),
            changed: false,
        }
    }

    fn remove_value(&mut self, value: &str) {
        let old = self.values.len();
        self.values.retain(|v| v != value);
        self.changed |= old != self.values.len();
    }

    fn replace_value(&mut self, old: &str, new: &str) {
        for arg in self.values.iter_mut() {
            if arg == old {
                *arg = new.to_string();
                self.changed = true;
            }
        }
    }

    fn read_response_file(path: &str) -> Result<Vec<String>> {
        let content = fs::read_to_string(path)?.replace("\r\n", "\n");
        Ok(Self::parse_response_file(content))
    }

    fn write_response_file(&self) -> Result<()> {
        let content: String = self
            .values
            .iter()
            .map(Self::escape)
            .collect::<Vec<String>>()
            .join(" ");

        fs::write(&self.new_path, content)?;
        Ok(())
    }

    fn escape(arg: &String) -> String {
        let mut result = String::new();
        let mut needs_quotes = false;

        for c in arg.chars() {
            match c {
                ' ' => {
                    needs_quotes = true;
                    result.push(c);
                }
                '\t' => {
                    needs_quotes = true;
                    result.push_str("\\t");
                }
                '"' => {
                    needs_quotes = true;
                    result.push_str("\\\"");
                }
                '\\' => {
                    result.push_str("\\\\");
                }
                _ => {
                    result.push(c);
                }
            }
        }

        if needs_quotes {
            format!("\"{}\"", result)
        } else {
            result
        }
    }

    fn unescape(arg: &str) -> String {
        let mut result = String::new();
        let mut chars = arg.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next_char) = chars.next() {
                    match next_char {
                        '"' => result.push('"'),
                        '\\' => result.push('\\'),
                        _ => {
                            result.push('\\');
                            result.push(next_char);
                        }
                    }
                } else {
                    result.push('\\');
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    // Function to parse the content of the response file into arguments
    fn parse_response_file(content: String) -> Vec<String> {
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut in_quotes = false;
        let mut escape_next = false;

        for c in content.chars() {
            if escape_next {
                current_arg.push(c);
                escape_next = false;
            } else if c == '\\' {
                escape_next = true;
            } else if c == '"' {
                in_quotes = !in_quotes;
            } else if c.is_whitespace() && !in_quotes {
                if !current_arg.is_empty() {
                    args.push(Self::unescape(&current_arg));
                    current_arg.clear();
                }
            } else {
                current_arg.push(c);
            }
        }

        if !current_arg.is_empty() {
            args.push(Self::unescape(&current_arg));
        }

        args
    }
}

struct Configuration {
    command: String,
    work_dir: String,
    just_print: bool,
    before_print: bool,
    redirect_stdout: String,
    redirect_stderr: String,
    arguments: Vec<String>,
    response_map: HashMap<String, ResponseFile>,
    log_file: String,
}

impl Configuration {
    fn new() -> Configuration {
        // 部分配置可以从环境变量读取默认值
        Configuration {
            command: get_string_environment_variable("CLW_OPT_COMMAND"),
            work_dir: "".to_string(),
            just_print: have_bool_environment_variable("CLW_OPT_JUST_PRINT"),
            before_print: have_bool_environment_variable("CLW_OPT_BEFORE_PRINT"),
            redirect_stdout: get_string_environment_variable("CLW_OPT_REDIRECT_STDOUT"),
            redirect_stderr: get_string_environment_variable("CLW_OPT_REDIRECT_STDERR"),
            arguments: vec![],
            response_map: HashMap::new(),
            log_file: get_string_environment_variable("CLW_LOG_FILE"),
        }
    }

    fn replace_response_file(&mut self) -> Result<()> {
        // 不支持嵌套 ResponseFile
        for (_, v) in self.response_map.iter() {
            if v.changed {
                v.write_response_file()?;
                let before = "@".to_string() + &v.original_path;
                let after = "@".to_string() + &v.new_path;

                for arg in self.arguments.iter_mut() {
                    if arg == &before {
                        *arg = after.clone();
                    }
                }
            }
        }
        Ok(())
    }
}

impl Drop for Configuration {
    fn drop(&mut self) {
        self.response_map
            .values()
            .map(|f| fs::remove_file(&f.new_path).unwrap_or(()))
            .count();
    }
}

fn change_link_feature(
    key: String,
    is_linker: Option<String>,
    dynamic_link: bool,
    mut is_dynamic: bool,
    arguments: &mut Vec<String>,
    response_map: &mut HashMap<String, ResponseFile>,
) -> bool {
    // 更改链接方式但不更改链接顺序, 因为有些情况下链接顺序很重要
    let (static_key, dynamic_key) = if is_linker.is_some() {
        ("-Bstatic".to_owned(), "-Bdynamic".to_owned())
    } else {
        ("-Wl,-Bstatic".to_owned(), "-Wl,-Bdynamic".to_owned())
    };
    let mut i = 0;
    while i < arguments.len() {
        let arg = arguments[i].clone();
        if arg == static_key || arg == "-dn" || arg == "-non_shared" || arg == "-static" {
            is_dynamic = false;
        } else if arg == dynamic_key || arg == "-dy" || arg == "-call_shared" {
            is_dynamic = true;
        } else if arg == key && is_dynamic != dynamic_link {
            if dynamic_link {
                arguments.insert(i, dynamic_key.clone());
                arguments.insert(i + 2, static_key.clone());
            } else {
                arguments.insert(i, static_key.clone());
                arguments.insert(i + 2, dynamic_key.clone());
            }
            i += 2;
        } else if let Some(path) = arg.strip_prefix("@") {
            if let Some(res) = response_map.get_mut(path) {
                let old_size = res.values.len();
                is_dynamic = change_link_feature(
                    key.clone(),
                    is_linker.clone(),
                    dynamic_link,
                    is_dynamic,
                    &mut res.values,
                    // 不支持嵌套 ResponseFile
                    &mut HashMap::new(),
                );
                if old_size != res.values.len() {
                    res.changed = true;
                }
            }
        }
        i += 1;
    }
    is_dynamic
}

fn static_link_feature(key: String, is_linker: Option<String>, arg: &mut Configuration) {
    change_link_feature(
        key,
        is_linker,
        false,
        true,
        &mut arg.arguments,
        &mut arg.response_map,
    );
}

fn dynamic_link_feature(key: String, is_linker: Option<String>, arg: &mut Configuration) {
    change_link_feature(
        key,
        is_linker,
        true,
        true,
        &mut arg.arguments,
        &mut arg.response_map,
    );
}

fn remove_argument(
    value: String,
    before: Option<String>,
    after: Option<String>,
    args: &mut Vec<String>,
    response_map: &mut HashMap<String, ResponseFile>,
) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    let mut i = 0;
    while i < args.len() {
        if let Some(path) = args[i].strip_prefix("@") {
            if let Some(res) = response_map.get_mut(path) {
                let elements = remove_argument(
                    value.clone(),
                    before.clone(),
                    after.clone(),
                    &mut res.values,
                    &mut HashMap::new(),
                );

                result.append(
                    &mut elements
                        .into_iter()
                        .filter(|item| !result.contains(item))
                        .collect::<Vec<String>>(),
                );
            }
        } else if args[i].ends_with(&value) {
            // 通常用于移动静态库/动态库在开头或末尾,因此这里仅匹配结尾字符串
            if let Some(ref before) = before {
                if i > 1 && args[i - 1].ends_with(before) {
                    let lib = args.remove(i);
                    if !result.contains(&lib) {
                        result.push(lib);
                    }
                    continue;
                }
            } else if let Some(ref after) = after {
                if i < args.len() - 1 && args[i + 1].ends_with(after) {
                    let lib = args.remove(i);
                    if !result.contains(&lib) {
                        result.push(lib);
                    }
                }
            } else {
                let lib = args.remove(i);
                if !result.contains(&lib) {
                    result.push(lib);
                }
            }
        }
        i += 1;
    }
    return result;
}

fn move_to_back_for_before_feature(value: String, before: Option<String>, arg: &mut Configuration) {
    // 将匹配的指定参数移动到末尾
    let mut result = remove_argument(
        value.clone(),
        before,
        None,
        &mut arg.arguments,
        &mut arg.response_map,
    );
    arg.arguments.append(&mut result);
}

fn move_to_back_for_after_feature(value: String, after: Option<String>, arg: &mut Configuration) {
    let mut result = remove_argument(
        value.clone(),
        None,
        after,
        &mut arg.arguments,
        &mut arg.response_map,
    );
    arg.arguments.append(&mut result);
}

fn move_to_front_for_before_feature(
    value: String,
    before: Option<String>,
    arg: &mut Configuration,
) {
    let result = remove_argument(
        value.clone(),
        before,
        None,
        &mut arg.arguments,
        &mut arg.response_map,
    );
    arg.arguments.splice(0..0, result.into_iter());
}

fn move_to_front_for_after_feature(value: String, after: Option<String>, arg: &mut Configuration) {
    let result = remove_argument(
        value.clone(),
        None,
        after,
        &mut arg.arguments,
        &mut arg.response_map,
    );
    arg.arguments.splice(0..0, result.into_iter());
}

fn replace_argument_feature(key: String, value: Option<String>, arg: &mut Configuration) {
    if let Some(value) = value {
        for arg in arg.arguments.iter_mut() {
            if arg == &key {
                *arg = value.clone();
            }
        }
        for (_, v) in arg.response_map.iter_mut() {
            v.replace_value(&key, &value)
        }
    }
}

fn remove_argument_feature(key: String, _: Option<String>, arg: &mut Configuration) {
    arg.arguments.retain(|item| item != &key);
    for (_, v) in arg.response_map.iter_mut() {
        v.remove_value(&key)
    }
}

fn have_bool_environment_variable(key: &str) -> bool {
    if let Ok(value) = env::var(key) {
        let v = value.to_lowercase();
        return v == "1" || v == "true" || v == "yes" || v == "on";
    }
    false
}

fn get_string_environment_variable(key: &str) -> String {
    env::var(key).unwrap_or("".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_to_back_feature() {
        let vec1: Vec<String> = vec![
            "a0".to_owned(),
            "prefix-a1".to_owned(),
            "prefix-a2".to_owned(),
            "a3".to_owned(),
            "a1".to_owned(),
            "a4".to_owned(),
            "a5".to_owned(),
        ];
        let mut config = Configuration::new();
        config.arguments = vec1.clone();
        move_to_back_for_after_feature("a1".to_owned(), None, &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "a0".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
                "prefix-a1".to_owned(),
                "a1".to_owned(),
            ]
        );

        config.arguments = vec1.clone();
        move_to_back_for_after_feature("a1".to_owned(), Some("a2".to_owned()), &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "a0".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a1".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
                "prefix-a1".to_owned(),
            ]
        );

        config.arguments = vec1.clone();
        move_to_back_for_after_feature("a5".to_owned(), Some("after".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);

        config.arguments = vec1.clone();
        move_to_back_for_before_feature("a1".to_owned(), Some("none".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);

        move_to_back_for_before_feature("a1".to_owned(), Some("a3".to_owned()), &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "a0".to_owned(),
                "prefix-a1".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
                "a1".to_owned(),
            ]
        );
        config.arguments = vec1.clone();
        move_to_back_for_before_feature("a0".to_owned(), Some("before".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);
    }

    #[test]
    fn test_move_to_before_feature() {
        let vec1: Vec<String> = vec![
            "a0".to_owned(),
            "prefix-a1".to_owned(),
            "prefix-a2".to_owned(),
            "a3".to_owned(),
            "a1".to_owned(),
            "a4".to_owned(),
            "a5".to_owned(),
        ];
        let mut config = Configuration::new();
        config.arguments = vec1.clone();
        move_to_front_for_after_feature("a1".to_owned(), None, &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "prefix-a1".to_owned(),
                "a1".to_owned(),
                "a0".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
            ]
        );

        config.arguments = vec1.clone();
        move_to_front_for_after_feature("a1".to_owned(), Some("a2".to_owned()), &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "prefix-a1".to_owned(),
                "a0".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a1".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
            ]
        );

        config.arguments = vec1.clone();
        move_to_front_for_after_feature("a5".to_owned(), Some("after".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);

        config.arguments = vec1.clone();
        move_to_front_for_before_feature("a1".to_owned(), Some("none".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);

        move_to_front_for_before_feature("a1".to_owned(), Some("a3".to_owned()), &mut config);
        assert_eq!(
            config.arguments,
            vec![
                "a1".to_owned(),
                "a0".to_owned(),
                "prefix-a1".to_owned(),
                "prefix-a2".to_owned(),
                "a3".to_owned(),
                "a4".to_owned(),
                "a5".to_owned(),
            ]
        );
        config.arguments = vec1.clone();
        move_to_front_for_before_feature("a0".to_owned(), Some("before".to_owned()), &mut config);
        assert_eq!(config.arguments, vec1);
    }
}

struct CommandWrapper(
    String,
    Option<String>,
    fn(String, Option<String>, &mut Configuration) -> (),
);

enum CommandType {
    Flag,
    Command(CommandWrapper),
    // 需要一个参数
    Option,
    Ignore,
}

fn parse_arguments(config: &mut Configuration, key: &str) -> CommandType {
    if key == "just-print" {
        config.just_print = true;
        CommandType::Flag
    } else if key == "before-print" {
        config.before_print = true;
        CommandType::Flag
    } else if let Some(log_file) = key.strip_prefix("log-file=") {
        config.log_file = log_file.to_string();
        CommandType::Option
    } else if let Some(command) = key.strip_prefix("command=") {
        config.command = command.to_string();
        CommandType::Option
    } else if let Some(dir) = key.strip_prefix("work-dir=") {
        config.work_dir = dir.to_string();
        CommandType::Option
    } else if let Some(path) = key.strip_prefix("redirect-stdout=") {
        config.redirect_stdout = path.to_string();
        CommandType::Option
    } else if let Some(path) = key.strip_prefix("redirect-stderr=") {
        config.redirect_stderr = path.to_string();
        CommandType::Option
    } else if let Some(arg) = key.strip_prefix("remove=") {
        CommandType::Command(CommandWrapper(
            arg.to_string(),
            None,
            remove_argument_feature,
        ))
    } else if let Some(arg) = key.strip_prefix("replace-") {
        let mut args = arg.splitn(2, '=');
        let before = args.next().unwrap_or("");
        let after = args.next().unwrap_or("");
        if before.is_empty() || after.is_empty() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                before.to_string(),
                Some(after.to_string()),
                replace_argument_feature,
            ))
        }
    } else if let Some(lib) = key.strip_prefix("static-link-compiler=") {
        CommandType::Command(CommandWrapper(lib.to_string(), None, static_link_feature))
    } else if let Some(lib) = key.strip_prefix("dynamic-link-compiler=") {
        CommandType::Command(CommandWrapper(lib.to_string(), None, dynamic_link_feature))
    } else if let Some(lib) = key.strip_prefix("static-link=") {
        CommandType::Command(CommandWrapper(
            lib.to_string(),
            Some("1".to_string()),
            static_link_feature,
        ))
    } else if let Some(lib) = key.strip_prefix("dynamic-link=") {
        CommandType::Command(CommandWrapper(
            lib.to_string(),
            Some("1".to_string()),
            dynamic_link_feature,
        ))
    } else if let Some(value) = key.strip_prefix("move-front=") {
        CommandType::Command(CommandWrapper(
            value.to_string(),
            None,
            move_to_front_for_before_feature,
        ))
    } else if let Some(value) = key.strip_prefix("move-front-before-") {
        let mut keys = value.splitn(2, '=');
        let before = keys.next().unwrap_or("");
        let value = keys.next().unwrap_or("");
        if before.is_empty() || value.is_empty() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                value.to_string(),
                Some(before.to_string()),
                move_to_front_for_before_feature,
            ))
        }
    } else if let Some(value) = key.strip_prefix("move-front-after-") {
        let mut keys = value.splitn(2, '=');
        let after = keys.next().unwrap_or("");
        let value = keys.next().unwrap_or("");
        if after.is_empty() || value.is_empty() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                value.to_string(),
                Some(after.to_string()),
                move_to_front_for_after_feature,
            ))
        }
    } else if let Some(value) = key.strip_prefix("move-back=") {
        CommandType::Command(CommandWrapper(
            value.to_string(),
            None,
            move_to_back_for_before_feature,
        ))
    } else if let Some(value) = key.strip_prefix("move-back-before-") {
        let mut keys = value.splitn(2, '=');
        let before = keys.next().unwrap_or("");
        let value = keys.next().unwrap_or("");
        if before.is_empty() || value.is_empty() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                value.to_string(),
                Some(before.to_string()),
                move_to_back_for_before_feature,
            ))
        }
    } else if let Some(value) = key.strip_prefix("move-back-after-") {
        let mut keys = value.splitn(2, '=');
        let after = keys.next().unwrap_or("");
        let value = keys.next().unwrap_or("");
        if after.is_empty() || value.is_empty() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                value.to_string(),
                Some(after.to_string()),
                move_to_back_for_after_feature,
            ))
        }
    } else {
        CommandType::Ignore
    }
}

fn run() -> Result<i32> {
    let mut config = Configuration::new();

    let exe = env::current_exe()
        .unwrap()
        .to_path_buf()
        .to_string_lossy()
        .to_string();

    // 默认可以走替换模式
    if config.command.is_empty() {
        let command = if let Some(value) = exe.strip_suffix(".exe") {
            value.to_owned() + "-wrapper.exe"
        } else {
            exe.to_owned() + "-wrapper"
        };
        if Path::new(&command).exists() {
            config.command = command;
        }
    }

    let prefix = "-clw-";
    let mut commands = vec![];
    let mut start_index = 1;
    if config.command.is_empty() {
        if env::args().len() < 2 {
            eprintln!("wrapper mode runs but no wrapper command is available");
            return Ok(1);
        }
        config.command = env::args().nth(1).unwrap();
        start_index = 2;
    }

    // 有些命令被驱动时可能没有环境变量,因此再增加配置文件读取,配置文件每一行一个命令
    let config_file_path = if let Some(value) = exe.strip_suffix(".exe") {
        value.to_owned() + "-clw-config.txt"
    } else {
        exe + "-clw-config.txt"
    };

    let config_file = Path::new(&config_file_path);

    if config_file.exists() {
        let content = fs::read_to_string(config_file)?.replace("\r\n", "\n");
        for argument in content.lines() {
            if let Some(key) = argument.strip_prefix(prefix) {
                match parse_arguments(&mut config, key) {
                    CommandType::Command(f) => commands.push(f),
                    CommandType::Ignore => {
                        config.arguments.push(argument.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    // 初始化 log
    init_log(config.log_file.as_str());

    {
        let mut iter = env::args().skip(start_index);
        while let Some(argument) = iter.next() {
            if let Some(key) = argument.strip_prefix(prefix) {
                match parse_arguments(&mut config, key) {
                    CommandType::Command(f) => commands.push(f),
                    CommandType::Ignore => {
                        config.arguments.push(argument);
                    }
                    _ => {}
                }
            } else if let Some(response_file) = argument.strip_prefix("@") {
                let path = Path::new(response_file);
                if path.exists() && path.is_file() {
                    let name = Path::new(response_file)
                        .file_name()
                        .unwrap()
                        .to_string_lossy();
                    let mut path = env::temp_dir();
                    path.push(format!("clw_res_{}", name));
                    config.response_map.insert(
                        response_file.to_string(),
                        ResponseFile::new(
                            response_file.to_string(),
                            path.to_string_lossy().into_owned(),
                        ),
                    );
                }
                config.arguments.push(argument);
            } else {
                config.arguments.push(argument);
            }
        }
    }

    for c in commands {
        c.2(c.0, c.1, &mut config);
    }

    config.replace_response_file()?;

    if config.just_print || config.before_print {
        warn!("{} {}", config.command, config.arguments.join(" "));
    }
    if config.just_print {
        return Ok(0);
    }

    let mut code = 1;

    let mut command = Command::new(&config.command);
    command.args(&config.arguments);
    if !config.work_dir.is_empty() {
        command.current_dir(&config.work_dir);
    }

    if config.redirect_stdout == config.redirect_stderr && !config.redirect_stdout.is_empty() {
        let output = File::create(&config.redirect_stdout)?;
        let error = output.try_clone()?;
        command.stdout(Stdio::from(output));
        command.stderr(Stdio::from(error));
    } else {
        if !config.redirect_stdout.is_empty() {
            let output = File::create(&config.redirect_stdout)?;
            command.stdout(Stdio::from(output));
        }
        if !config.redirect_stderr.is_empty() {
            let error = File::create(&config.redirect_stderr)?;
            command.stderr(Stdio::from(error));
        }
    }

    match command.spawn() {
        Ok(mut child) => {
            let exit_status = child.wait().expect("Failed to wait for child");
            code = exit_status.code().unwrap_or(4);
        }
        Err(e) => {
            error!("Failed to execute command: {}", e);
        }
    }
    Ok(code)
}

fn init_log(log_file: &str) {
    let log_level = get_string_environment_variable("RUST_LOG").to_lowercase();
    let level = match log_level.as_str() {
        "off" => LevelFilter::Off,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    if log_file.is_empty() {
        TermLogger::init(
            level,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )
        .unwrap();
    } else {
        CombinedLogger::init(vec![WriteLogger::new(
            level,
            Config::default(),
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)
                .unwrap(),
        )])
        .unwrap();
    }
}

fn main() {
    match run() {
        Ok(code) => {
            if code != 0 {
                std::process::exit(code);
            }
        }
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        }
    };
}

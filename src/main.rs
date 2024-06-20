use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
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
}

impl Configuration {
    fn new() -> Configuration {
        Configuration {
            command: "".to_string(),
            work_dir: "".to_string(),
            just_print: false,
            before_print: false,
            redirect_stdout: "".to_string(),
            redirect_stderr: "".to_string(),
            arguments: vec![],
            response_map: HashMap::new(),
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

fn static_link_feature(key: String, is_linker: Option<String>, arg: &mut Configuration) {
    if is_linker.is_some() {
        remove_argument_feature(key.clone(), None, arg);
        arg.arguments.push("-Bstatic".to_string());
        arg.arguments.push(key)
    } else {
        remove_argument_feature("-Wl,".to_string() + &key, None, arg);
        arg.arguments.push("-Wl,-Bstatic".to_string());
        arg.arguments.push("-Wl,".to_string() + &key);
    }
}

fn dynamic_link_feature(key: String, is_linker: Option<String>, arg: &mut Configuration) {
    if is_linker.is_some() {
        remove_argument_feature(key.clone(), None, arg);
        arg.arguments.push("-Bdynamic".to_string());
        arg.arguments.push(key)
    } else {
        remove_argument_feature("-Wl,".to_string() + &key, None, arg);
        arg.arguments.push("-Wl,-Bdynamic".to_string());
        arg.arguments.push("-Wl,".to_string() + &key);
    }
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
        let after = args.next();
        if after.is_none() {
            CommandType::Ignore
        } else {
            CommandType::Command(CommandWrapper(
                before.to_string(),
                Some(after.unwrap().to_string()),
                replace_argument_feature,
            ))
        }
    } else if let Some(lib) = key.strip_prefix("static-link-compiler=") {
        CommandType::Command(CommandWrapper(
            lib.to_string(),
            Some("1".to_string()),
            static_link_feature,
        ))
    } else if let Some(lib) = key.strip_prefix("dynamic-link-compiler=") {
        CommandType::Command(CommandWrapper(
            lib.to_string(),
            Some("1".to_string()),
            dynamic_link_feature,
        ))
    } else if let Some(lib) = key.strip_prefix("static-link=") {
        CommandType::Command(CommandWrapper(lib.to_string(), None, static_link_feature))
    } else if let Some(lib) = key.strip_prefix("dynamic-link=") {
        CommandType::Command(CommandWrapper(lib.to_string(), None, dynamic_link_feature))
    } else {
        CommandType::Ignore
    }
}

fn run() -> Result<i32> {
    if env::args().len() == 1 {
        eprintln!("No wrapping command specified");
        return Ok(2);
    }
    let prefix = "-clw-";
    let mut config = Configuration::new();
    config.command = env::args().nth(1).unwrap();
    let mut commands = vec![];

    {
        let mut iter = env::args().skip(2);
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
        println!("{} {}", config.command, config.arguments.join(" "));
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
            eprintln!("Failed to execute command: {}", e);
        }
    }
    Ok(code)
}

fn main() {
    match run() {
        Ok(code) => {
            std::process::exit(code);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
}

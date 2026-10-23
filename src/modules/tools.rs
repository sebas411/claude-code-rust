use std::{fs::{self, File}, io::Write, path::Path, process::Command};

use anyhow::{Result, anyhow};
use async_openai::config::OpenAIConfig;
use regex::{Captures, Regex};
use serde_json::{Value, json};

use crate::modules::agent::Agent;

pub fn replace_args(body: &str, args: Vec<&str>) -> String {
    let re = Regex::new(r"\$(?:ARGUMENTS\[(\d+)\]|(\d+))").unwrap();
    let current_string = re.replace_all(body, |caps: &Captures<'_>| {
        let n_str = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
        let n = n_str.parse::<usize>().unwrap();
        if args.len() > n {
            args[n]
        } else {
            ""
        }
    }).to_string();
    current_string.replace("$ARGUMENTS", &args.join(" "))
}

pub struct ToolHandler {
    tool_list: Value
}

impl ToolHandler {
    pub fn new() -> Self {
        let tool_list = json!([
            {
                "type": "function",
                "function": {
                    "name": "Read",
                    "description": "Read and return the contents of a file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path to the file to read"
                            }
                        },
                        "required": ["file_path"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "Write",
                    "description": "Write content to a file",
                    "parameters": {
                        "type": "object",
                        "required": ["file_path", "content"],
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path of the file to write to"
                            },
                            "content": {
                                "type": "string",
                                "description": "The content to write to the file"
                            }
                        }
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "Bash",
                    "description": "Execute a shell command",
                    "parameters": {
                        "type": "object",
                        "required": ["command"],
                        "properties": {
                            "command": {
                            "type": "string",
                            "description": "The command to execute"
                            }
                        }
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "Skill",
                    "description": "Load a skill's instructions into the conversation",
                    "parameters": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": { "type": "string", "description": "The name of the skill to use" },
                            "args": { "type": "string", "description": "Optional arguments for the skill" }
                        }
                    }
                }
            },
        ]);
        Self { tool_list }
    }

    pub fn get_tools(&self) -> Value {
        self.tool_list.clone()
    }

    pub async fn call_tool(&self, name: &str, call_id: Value, arguments: &str, config: OpenAIConfig) -> Result<Value> {
        match name {
            "Read" => {
                let arguments = serde_json::from_str::<Value>(arguments).unwrap();
                let filepath = Path::new(arguments["file_path"].as_str().unwrap());
                if !filepath.exists() {
                    return Ok(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": "ERR(Read tool): file doesn't exist."
                    }))
                }
                if filepath.is_dir() {
                    return Ok(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": "ERR(Read tool): path provided is a directory, cannot read."
                    }))
                }
                match fs::read_to_string(filepath) {
                    Ok(contents) => {
                        let response = json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": contents
                        });
                        return Ok(response);
                    }
                    Err(_) => {
                        eprintln!("File cannot be read as a string")
                    }
                }
            },
            "Write" => {
                let arguments = serde_json::from_str::<Value>(arguments).unwrap();
                let filepath = arguments["file_path"].as_str().unwrap();
                let content = arguments["content"].as_str().unwrap();
                let mut file = File::create(filepath)?;
                file.write_all(content.as_bytes())?;
                let response = json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": "Contents written."
                });
                return Ok(response);
            },
            "Bash" => {
                let arguments = serde_json::from_str::<Value>(arguments).unwrap();
                let command = arguments["command"].as_str().unwrap();
                let output = Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .expect("failed to execute process");
                let stdout = String::from_utf8(output.stdout)?;
                let stderr = String::from_utf8(output.stderr)?;
                let status = output.status;
                let out;
                if status.success() {
                    out = stdout;
                } else {
                    out = stderr;
                }
                let response = json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": out
                });
                return Ok(response);
            },
            "Skill" => {
                let arguments_values = serde_json::from_str::<Value>(arguments).unwrap();
                let arguments = arguments_values.as_object().ok_or(anyhow!("Arguments are not a json object"))?;
                let name = arguments["name"].as_str().unwrap();
                let mut skill_args = String::new();
                if arguments.contains_key("args") {
                    skill_args = arguments["args"].as_str().unwrap_or_default().to_string();
                }
                let contents = fs::read_to_string(format!(".claude/skills/{}/SKILL.md", name))?;
                let frontmatter = serde_yaml::from_str::<serde_yaml::Value>(contents.split("---\n").nth(1).unwrap_or_default())?;
                let mut body = replace_args(&contents.split("---\n").nth(2).unwrap_or_default(), skill_args.split(' ').collect());
                if let Some(context) = frontmatter.get("context") && context.as_str().unwrap_or_default() == "fork" {
                    let mut subagent = Agent::new_with_config(config)?;
                    body = Box::pin(subagent.handle_request(&body)).await?;
                }
                return Ok(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": body
                }))
            }
            _ => ()
        }
        Err(anyhow!("Model requested unavailable tool"))
    }
}

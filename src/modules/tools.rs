use std::{fs::{self, File}, io::Write, process::Command};

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

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
            }
        ]);
        Self { tool_list }
    }

    pub fn get_tools(&self) -> Value {
        self.tool_list.clone()
    }

    pub fn call_tool(&self, name: &str, call_id: Value, arguments: &str) -> Result<Value> {
        match name {
            "Read" => {
                let arguments = serde_json::from_str::<Value>(arguments).unwrap();
                let filepath = arguments["file_path"].as_str().unwrap();
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
            _ => ()
        }
        Err(anyhow!("Model requested unavailable tool"))
    }
}

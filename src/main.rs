use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde_json::{Value, json};
use std::{env, fs, process};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    let mut messages = vec![json!(
        {
            "role": "user",
            "content": args.prompt
        }
    )];
    loop {
        let response: Value = client
            .chat()
            .create_byot(json!({
                "messages": messages,
                "model": "anthropic/claude-haiku-4.5",
                "tools": [
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
                ],
            }))
            .await?;

        let message = response["choices"][0]["message"].clone();
    
        if let Some(calls) = message["tool_calls"].as_array() {
            let call_id = calls[0]["id"].clone();
            let function = &calls[0]["function"].as_object().unwrap();
            let name = function["name"].as_str().unwrap();
            let arguments = function["arguments"].as_str().unwrap();
            messages.push(message.clone());
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
                            messages.push(response);
                        }
                        Err(_) => {
                            eprintln!("File cannot be read as a string")
                        }
                    }
                },
                _ => {
                    eprintln!("Model requested unavailable function");
                    process::exit(1);
                }
            }
        } else {
            if let Some(content) = message["content"].as_str() {
                println!("{}", content);
            }
            break;
        }
    }

    Ok(())
}

use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde_json::{Value, json};
use std::{env, process};

use crate::modules::tools::ToolHandler;

mod modules;

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

    let tool_handler = ToolHandler::new();

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
                "tools": &tool_handler.get_tools(),
            }))
            .await?;

        let message = response["choices"][0]["message"].clone();
    
        if let Some(calls) = message["tool_calls"].as_array() {
            let call_id = calls[0]["id"].clone();
            let function = &calls[0]["function"].as_object().unwrap();
            let name = function["name"].as_str().unwrap();
            let arguments = function["arguments"].as_str().unwrap();
            messages.push(message.clone());
            let response = tool_handler.call_tool(name, call_id, arguments)?;
            messages.push(response);
        } else {
            if let Some(content) = message["content"].as_str() {
                println!("{}", content);
            }
            break;
        }
    }

    Ok(())
}

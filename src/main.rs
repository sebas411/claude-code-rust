use anyhow::anyhow;
use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use regex::{Captures, Regex};
use serde_json::{Value, json};
use std::{env, fs, path::Path, process};

use crate::modules::tools::ToolHandler;

mod modules;

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

fn replace_args(body: &str, args: Vec<&str>) -> String {
    let re = Regex::new(r"\$(?:ARGUMENTS\[(\d+)\]|(\d+))").unwrap();
    let current_string = re.replace_all(body, |caps: &Captures<'_>| {
        let n_str = caps.get(1).or_else(|| caps.get(2)).unwrap().as_str();
        let n = n_str.parse::<usize>().unwrap();
        if args.len() >= n {
            args[n]
        } else {
            ""
        }
    }).to_string();
    current_string.replace("$ARGUMENTS", &args.join(" "))
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
    
    let mut skills = vec![];
    let mut system_prompt = "You have access to the following skills:\n\n".to_string();
    let skills_path = Path::new(".claude/skills");
    if skills_path.is_dir() {
        for skill in fs::read_dir(skills_path)? {
            let skill_entry = skill?;
            let skill_path = skill_entry.path();
            let filename = skill_path.join("SKILL.md");
            if filename.exists() {
                let contents = fs::read_to_string(filename)?;
                let frontmatter = contents.split("---\n").nth(1).ok_or(anyhow!("Frontmatter missing from file"))?;
                let frontmatter_yaml = serde_yaml::from_str::<serde_yaml::Value>(frontmatter)?;
                let frontmatter_yaml = frontmatter_yaml.as_mapping().ok_or("Value is not mapping")?;
                let name = frontmatter_yaml.get("name").ok_or("name field not found")?.as_str().unwrap();
                let description = frontmatter_yaml.get("description").ok_or("name field not found")?.as_str().unwrap();
                skills.push(name.to_string());
                system_prompt.push_str(&format!("- {}: {}\n", name, description));
            }
        }
    }
    
    let user_prompt;
    if args.prompt.starts_with('/') && skills.contains(&args.prompt.split(' ').nth(0).unwrap().strip_prefix('/').unwrap().to_string()) {
        let contents = fs::read_to_string(format!(".claude/skills/{}/SKILL.md", args.prompt.split(' ').nth(0).unwrap().strip_prefix('/').unwrap()))?;
        let body = contents.split("---").nth(2).ok_or(anyhow!("Skill contains no body"))?.to_string();

        let arguments = args.prompt.split(' ').skip(1).collect::<Vec<&str>>();
        user_prompt = replace_args(&body, arguments);
    } else {
        user_prompt = args.prompt;
    }

    let mut messages = vec![
        json!({
            "role": "system",
            "content": system_prompt
        }), 
        json!({
            "role": "user",
            "content": user_prompt
        })
    ];

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

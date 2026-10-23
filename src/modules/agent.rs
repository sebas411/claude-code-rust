use std::{fs, path::Path};

use anyhow::{Result, anyhow};
use async_openai::{Client, config::OpenAIConfig};
use serde_json::{Value, json};

use crate::modules::tools::{ToolHandler, replace_args};

pub struct Agent {
    config: OpenAIConfig,
    messages: Vec<Value>,
    skills: Vec<String>,
}

impl Agent {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let config = OpenAIConfig::new()
            .with_api_base(base_url)
            .with_api_key(api_key);
        Agent::new_with_config(config)
    }

    pub fn new_with_config(config: OpenAIConfig) -> Result<Self> {
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
                    let frontmatter_yaml = frontmatter_yaml.as_mapping().ok_or(anyhow!("Value is not mapping"))?;
                    let name = frontmatter_yaml.get("name").ok_or(anyhow!("name field not found"))?.as_str().unwrap();
                    let description = frontmatter_yaml.get("description").ok_or(anyhow!("name field not found"))?.as_str().unwrap();
                    skills.push(name.to_string());
                    system_prompt.push_str(&format!("- {}: {}\n", name, description));
                }
            }
        }
        let messages = vec![json!({
            "role": "system",
            "content": system_prompt
        })];
        Ok(Self { config, messages, skills })
    }

    pub async fn handle_request(&mut self, request: &str) -> Result<String> {
        let client = Client::with_config(self.config.clone());
        let tool_handler = ToolHandler::new();

        let mut used_skills = vec![];
        if request.starts_with('/') && self.skills.contains(&request.split(' ').nth(0).unwrap().strip_prefix('/').unwrap().to_string()) {
            let current_skill = request.split(' ').nth(0).unwrap().strip_prefix('/').unwrap();
            used_skills.push(current_skill.to_string());
            let mut arguments = request.split(' ').skip(1).collect::<Vec<&str>>();
            
            while arguments.len() > 0 && arguments[0].starts_with('/') && self.skills.contains(&arguments[0].strip_prefix('/').unwrap().to_string()) {
                used_skills.push(arguments.remove(0).strip_prefix('/').unwrap().to_string());
            }
            
            for skill in used_skills {
                let contents = fs::read_to_string(format!(".claude/skills/{}/SKILL.md", skill))?;
                let body = contents.split("---").nth(2).ok_or(anyhow!("Skill contains no body"))?.to_string();
                let complete_message = format!("Skill: {} (this skill's directory is: .claude/skills/{}/)\nPaths in the instructions below are relative to that folder.\n\n{}", skill, skill, replace_args(&body, arguments.clone()));
                self.messages.push(json!({
                    "role": "user",
                    "content": complete_message
                }))
            }
        } else {
            self.messages.push(json!({
                "role": "user",
                "content": request
            }))
        }

        loop {
            let response: Value = client
                .chat()
                .create_byot(json!({
                    "messages": self.messages,
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
                self.messages.push(message.clone());
                let response = tool_handler.call_tool(name, call_id, arguments, self.config.clone()).await?;
                self.messages.push(response);
            } else {
                if let Some(content) = message["content"].as_str() {
                    return Ok(content.to_string())
                }
            }
        }
    }
}
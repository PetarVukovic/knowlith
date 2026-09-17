//! Multi-turn supervisor session backed by the lake, not a hung child process.

use knowlith_engine::{Engine, Request};
use knowlith_lake::Lake;

pub struct Session {
    pub id: String,
    turns: Vec<(String, String)>,
}

impl Session {
    pub fn load(lake: &Lake, id: &str) -> Result<Self, String> {
        Ok(Self {
            id: id.to_string(),
            turns: lake.supervisor_turns(id).map_err(|e| e.to_string())?,
        })
    }

    pub fn record(&self, lake: &Lake, role: &str, content: &str) -> Result<(), String> {
        lake.append_supervisor_turn(&self.id, role, content)
            .map_err(|e| e.to_string())
    }

    pub fn ask(
        &mut self,
        engine: &dyn Engine,
        user: &str,
        instructions: &str,
        schema: Option<&str>,
    ) -> Result<String, String> {
        let mut input = String::new();
        for (role, content) in &self.turns {
            input.push_str(&format!("[{role}]\n{content}\n\n"));
        }
        input.push_str(user);
        let mut request = Request::new("supervisor", instructions, input);
        if let Some(schema) = schema {
            request = request.with_schema(schema);
        }
        let reply = engine.run(&request).map_err(|e| e.to_string())?;
        self.turns.push(("user".into(), user.to_string()));
        self.turns.push(("assistant".into(), reply.text.clone()));
        Ok(reply.text)
    }
}

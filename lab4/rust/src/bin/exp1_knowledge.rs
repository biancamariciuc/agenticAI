//! Lab 4 - Experiment 1: what does the model actually know? (given, nothing to complete)
use genai::Client;
use lab4_rs::llm::{ask, LlmError};

const PROBES: [&str; 4] = [
    "What is today's date?",
    "What is the cheapest flight from Iasi to Rome next Tuesday, and how much does it cost?",
    "Which concerts are on in Rome in the second week of May 2027?",
    "Is the Borghese Gallery in Rome open on Mondays, and how do you get tickets?",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let client = Client::new()?;
    for q in PROBES {
        match ask(&client, None, q, None).await {
            Ok(r) => {
                let short: String = r.text.trim().chars().take(600).collect();
                println!("\n### {q}\n{short}");
            }
            // every next call would fail the same way: stop
            Err(LlmError::Quota(m)) => {
                println!("\n[stopped] {m}");
                std::process::exit(1);
            }
            // one slow answer: report it and go on
            Err(LlmError::Timeout(m)) => println!("\n### {q}\n[timeout] {m}"),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

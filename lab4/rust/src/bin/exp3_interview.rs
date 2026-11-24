//! Lab 4 - Experiment 3: ask before you plan (STARTER).
//!   cargo run --bin exp3_interview               # you answer the questions
//!   cargo run --bin exp3_interview -- --vague    # step A: what happens when nobody asks
//!   cargo run --bin exp3_interview -- --simulate # a simulated traveler with a hidden profile answers
use genai::Client;
use lab4_rs::llm::{ask, ask_json};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

const FIRST_MESSAGE: &str = "Plan me a nice 4-day trip in May.";
const MAX_QUESTIONS: usize = 8;

#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
struct TravelBrief {
    /// City the trip starts from
    origin: Option<String>,
    /// City or region, or 'open' if the traveler wants suggestions
    destination: Option<String>,
    /// Arrival and departure dates, e.g. '12-16 May'
    dates: Option<String>,
    /// How many people travel
    travelers: Option<u32>,
    /// Total budget for the whole trip, in EUR
    budget_eur: Option<u32>,
    /// What they enjoy doing
    interests: Vec<String>,
    /// relaxed, balanced or packed
    pace: Option<String>,
    /// Things they do not want
    avoid: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct InterviewTurn {
    /// Everything learned so far; unknown fields stay null
    brief: TravelBrief,
    /// ONE short question about the most important unknown, or '' when done
    next_question: String,
}

// TODO: write the interviewer's system prompt. It must NOT plan, must NOT guess or invent defaults,
// must fill the brief only with what the traveler said, and must ask ONE question at a time
// about the most important unknown (empty next_question when nothing is missing).
const INTERVIEWER: &str = "TODO";

/// The harness decides when the brief is complete, not the model.
fn missing_required(b: &TravelBrief) -> Vec<&'static str> {
    // TODO: return the names of the REQUIRED fields that are still empty:
    // origin, destination, dates, travelers, budget_eur, interests
    let _ = b;
    Vec::new()
}

const PERSONA: &str = "You are Ana, a student from Iasi. Hidden facts about your trip: 2 people (you and a friend), \
12-16 May, total budget 1000 EUR, you love street food and old history, you hate early mornings, \
you have no destination in mind and want a suggestion in Italy. \
Answer ONLY the question asked, in one short sentence, like a real person would. Never volunteer extra facts.";

async fn answer(client: &Client, simulate: bool, transcript: &str, question: &str) -> Result<String, String> {
    if simulate {
        let prompt = format!("Conversation so far:\n{transcript}\nThe planner asks: {question}\nYour reply:");
        return Ok(ask(client, Some(PERSONA), &prompt, Some(0.7)).await?.text.trim().to_string());
    }
    print!("> ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line).ok();
    Ok(line.trim().to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let client = Client::new()?;
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--vague") {
        println!("{}", ask(&client, None, FIRST_MESSAGE, None).await?.text);
        return Ok(());
    }
    let simulate = args.iter().any(|a| a == "--simulate");

    println!("[Traveler] {FIRST_MESSAGE}");
    let mut transcript = format!("Traveler: {FIRST_MESSAGE}");
    let mut brief = TravelBrief::default();
    let mut asked = 0;
    while asked < MAX_QUESTIONS {
        let (turn, _): (InterviewTurn, _) =
            ask_json(&client, Some(INTERVIEWER), &format!("Conversation so far:\n{transcript}"), None).await?;
        brief = turn.brief;
        let _missing = missing_required(&brief);
        // TODO: stop (break) when the HARNESS says the brief is complete (`missing` is empty),
        // whatever the model put in next_question. If the model returns an empty question
        // while something is still missing, ask about missing[0] yourself.
        let question = turn.next_question;
        println!("\n[Planner] {question}");
        let reply = answer(&client, simulate, &transcript, &question).await?;
        println!("[Traveler] {reply}");
        transcript.push_str(&format!("\nPlanner: {question}\nTraveler: {reply}"));
        asked += 1;
    }
    let missing = missing_required(&brief);
    println!("\n[Brief after {asked} questions] missing: {}", if missing.is_empty() { "nothing".into() } else { missing.join(", ") });
    println!("{}", serde_json::to_string_pretty(&brief)?);
    Ok(())
}

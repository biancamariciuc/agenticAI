//! Lab 4 - Experiment 5: three ways to use the same tools (STARTER).  Tests: cargo test --bin exp5_tools
//!   cargo run --bin exp5_tools                    # all 4 questions x 3 modes, one run each
//!   cargo run --bin exp5_tools -- loop 3          # one mode on one question
//!   cargo run --bin exp5_tools -- --runs 3        # repeat every cell 3 times
//!   cargo run --bin exp5_tools -- --verbose       # print every tool call (loop) and every plan
//!   cargo run --bin exp5_tools -- --sum           # the loop also gets a sum_eur tool
use genai::chat::Tool;
use genai::Client;
use lab4_rs::agent_core::{arg_str, no_approval, run_agent, tool, AgentConfig, ToolHost};
use lab4_rs::llm::{ask, ask_json};
use regex::Regex;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

static TOOL_LATENCY_MS: AtomicU64 = AtomicU64::new(1000); // pretend every tool is a slow web API
static TOOL_WAIT_MS: AtomicU64 = AtomicU64::new(0); // total waiting on tools (one at a time)

async fn wait() {
    let ms = TOOL_LATENCY_MS.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(ms)).await;
    TOOL_WAIT_MS.fetch_add(ms, Ordering::Relaxed);
}

/// "Rome (FCO)", " rome " and "ROME" all mean rome: real tools normalise their input.
fn city(name: &str) -> String {
    let base = name.split('(').next().unwrap_or(name);
    base.trim().to_lowercase()
}

// --- Tools (deterministic, slow on purpose) ---
async fn nearest_airport(town: &str) -> String {
    wait().await;
    match city(town).as_str() {
        "vaslui" | "iasi" => "Iasi".into(),
        "botosani" => "Suceava".into(),
        _ => format!("Unknown town {town}."),
    }
}

async fn flight_price(origin: &str, destination: &str, date: &str) -> String {
    wait().await;
    let price = match (city(origin).as_str(), city(destination).as_str()) {
        ("iasi", "rome") => Some(142),
        ("iasi", "barcelona") => Some(118),
        ("iasi", "bucharest") => Some(39),
        ("bucharest", "lisbon") => Some(167),
        ("suceava", "rome") => Some(131),
        ("suceava", "barcelona") => Some(156),
        _ => None,
    };
    match price {
        Some(p) => json!({"route": format!("{origin}->{destination}"), "date": date, "price_eur": p}).to_string(),
        None if city(destination) == "lisbon" => {
            "No direct flight Iasi-Lisbon. Connect via Bucharest: price Iasi->Bucharest and Bucharest->Lisbon.".into()
        }
        None => format!("No flights found {origin}->{destination}."),
    }
}

async fn hotel_price(name: &str, nights: u64) -> String {
    wait().await;
    let per_night = match city(name).as_str() {
        "rome" => 96,
        "barcelona" => 121,
        "lisbon" => 74,
        _ => return format!("No hotels found in {name}."),
    };
    json!({"city": name, "nights": nights, "total_eur": per_night * nights}).to_string()
}

async fn weather(name: &str) -> String {
    wait().await;
    match city(name).as_str() {
        "rome" => "24 C, sunny".into(),
        "barcelona" => "22 C, cloudy".into(),
        "lisbon" => "21 C, windy".into(),
        _ => format!("No weather data for {name}."),
    }
}

/// One dispatcher used by both the loop and the plan executor. Arguments arrive as JSON.
async fn dispatch(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "nearest_airport" => Ok(nearest_airport(arg_str(args, "town")?).await),
        "flight_price" => Ok(flight_price(arg_str(args, "origin")?, arg_str(args, "destination")?, arg_str(args, "date")?).await),
        "hotel_price" => {
            let nights = args["nights"].as_u64().or_else(|| args["nights"].as_str()?.parse().ok()).ok_or("nights must be a number")?;
            Ok(hotel_price(arg_str(args, "city")?, nights).await)
        }
        "weather" => Ok(weather(arg_str(args, "city")?).await),
        // TODO (homework C, step 2): "sum_eur" adds up args["amounts"] (a JSON array of numbers) exactly.
        "sum_eur" => Err("TODO: sum_eur not implemented".into()),
        other => Err(format!("Unknown tool '{other}'.")),
    }
}

const QUESTIONS: [(&str, &str); 4] = [
    ("From Iasi on 12 May: is Rome or Barcelona cheaper for one person, flight plus 3 hotel nights?", "430"),
    ("What is the weather right now in Rome, Barcelona and Lisbon?", "windy"),
    ("What does it cost one person to get from Iasi to Lisbon on 12 May, plus 3 hotel nights?", "428"),
    ("I live in Vaslui. On 12 May, is it cheaper to fly from my nearest airport to Rome or to Barcelona?", "118"),
];
const SYSTEM: &str = "You are a travel assistant. Use the tools for every price and every forecast; never guess numbers.";

// --- Mode 1: the loop you built in Lab 3 ---
struct TravelTools {
    with_sum: bool,
}

impl ToolHost for TravelTools {
    fn tools(&self) -> Vec<Tool> {
        let s = json!({"type": "string"});
        let mut tools = vec![
            tool("nearest_airport", "Only for Romanian towns WITHOUT an airport (e.g. Vaslui): returns just the city of the nearest airport.                   Never use it for a city that has an airport, such as Iasi, Rome or Barcelona.",
                 json!({"type": "object", "properties": {"town": s}, "required": ["town"]})),
            tool("flight_price", "Cheapest one-way economy ticket for one adult, in EUR.",
                 json!({"type": "object", "properties": {"origin": s, "destination": s, "date": s}, "required": ["origin", "destination", "date"]})),
            tool("hotel_price", "Total price in EUR of a double room in a central 3-star hotel.",
                 json!({"type": "object", "properties": {"city": s, "nights": {"type": "integer"}}, "required": ["city", "nights"]})),
            tool("weather", "Current weather in a city.",
                 json!({"type": "object", "properties": {"city": s}, "required": ["city"]})),
        ];
        if self.with_sum {
            tools.push(tool("sum_eur", "Add up a list of EUR amounts exactly. Use it for every total.",
                json!({"type": "object", "properties": {"amounts": {"type": "array", "items": {"type": "number"}}}, "required": ["amounts"]})));
        }
        tools
    }

    async fn call(&mut self, name: &str, args: &Value) -> Result<String, String> {
        dispatch(name, args).await
    }
}

async fn mode_loop(client: &Client, question: &str, with_sum: bool, verbose: bool) -> (String, usize, i32, f64) {
    let system = if with_sum { format!("{SYSTEM} Never add numbers yourself: use sum_eur.") } else { SYSTEM.to_string() };
    let mut cfg = AgentConfig::new(&system);
    cfg.verbose = verbose;
    TOOL_WAIT_MS.store(0, Ordering::Relaxed);
    let res = run_agent(client, &cfg, &mut TravelTools { with_sum }, &mut no_approval(), question).await;
    let tokens = res.total_prompt_tokens();
    // the loop runs tools one by one, so the waiting adds up
    (res.answer, res.steps, tokens, TOOL_WAIT_MS.load(Ordering::Relaxed) as f64 / 1000.0)
}

// --- Modes 2 and 3: plan once, run the tools in code, answer once ---
#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct Arg {
    name: String,
    /// A literal value, or @s1 to use the result of step s1 (only when that result is a single value)
    value: String,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ToolName {
    NearestAirport,
    FlightPrice,
    HotelPrice,
    Weather,
}

impl ToolName {
    fn as_str(self) -> &'static str {
        match self {
            ToolName::NearestAirport => "nearest_airport",
            ToolName::FlightPrice => "flight_price",
            ToolName::HotelPrice => "hotel_price",
            ToolName::Weather => "weather",
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct Step {
    /// s1, s2, ...
    id: String,
    #[allow(dead_code)]
    why: String,
    tool: ToolName,
    args: Vec<Arg>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct Plan {
    steps: Vec<Step>,
}

const PLANNER: &str = "Write the COMPLETE list of tool calls needed to answer, before any tool runs. \
Tools: nearest_airport(town) (only for towns without an airport), flight_price(origin, destination, date), hotel_price(city, nights), weather(city). \
Give each step an id s1, s2, ... A later step may use an earlier step's result by writing @s1 as an argument value.";
const SOLVER: &str = "Answer the question using ONLY the evidence below. If the evidence is not enough, say exactly what is missing.";

fn refs() -> Regex {
    Regex::new(r"@s\d+").unwrap()
}

/// Replace @sN references with earlier results, then call the tool. Errors become results.
async fn run_step(step: Step, results: HashMap<String, String>) -> (String, String) {
    let re = refs();
    let mut args = serde_json::Map::new();
    for a in &step.args {
        let v = re.replace_all(&a.value, |c: &regex::Captures| results.get(&c[0][1..]).cloned().unwrap_or(c[0].to_string()));
        args.insert(a.name.clone(), Value::String(v.into_owned()));
    }
    let out = dispatch(step.tool.as_str(), &Value::Object(args)).await.unwrap_or_else(|e| format!("ERROR: {e}"));
    (step.id, out)
}

/// Run the plan; with `parallel`, every step whose inputs are ready runs at the same time.
async fn execute(plan: &Plan, parallel: bool) -> HashMap<String, String> {
    let results: HashMap<String, String> = HashMap::new();
    // TODO (homework C, step 1): run every step of plan.steps with run_step(step, results.clone()) and store
    // the output in results[step.id]. A step is READY when every @s reference in its args has a result.
    //   parallel = false -> run the ready steps one by one
    //   parallel = true  -> spawn all ready steps on a tokio JoinSet, collect them, then look again
    // If nothing is ready (a reference to a step that does not exist), run the first pending step as-is.
    // The tests at the bottom of this file check all of this without calling the model.
    let _ = (plan, parallel, refs(), JoinSet::<()>::new());
    results
}

async fn mode_plan(client: &Client, question: &str, parallel: bool, verbose: bool) -> Result<(String, usize, i32, f64), String> {
    let (plan, r1): (Plan, _) = ask_json(client, Some(&format!("{SYSTEM} {PLANNER}")), question, None).await?;
    let t0 = Instant::now();
    let results = execute(&plan, parallel).await;
    let tool_seconds = t0.elapsed().as_secs_f64();
    let lines: Vec<String> = plan.steps.iter().map(|s| {
        let args: Vec<String> = s.args.iter().map(|a| format!("{}={}", a.name, a.value)).collect();
        format!("{} = {}({}) -> {}", s.id, s.tool.as_str(), args.join(", "), results.get(&s.id).map_or("", |v| v))
    }).collect();
    if verbose {
        for l in &lines {
            println!("   {l}");
        }
    }
    let r2 = ask(client, Some(SOLVER), &format!("Question: {question}\nEvidence:\n{}", lines.join("\n")), None).await?;
    Ok((r2.text, 2, r1.in_tokens + r2.in_tokens, tool_seconds))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let client = Client::new()?;
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let (with_sum, verbose) = (argv.iter().any(|a| a == "--sum"), argv.iter().any(|a| a == "--verbose"));
    let mut runs = 1;
    if let Some(i) = argv.iter().position(|a| a == "--runs") {
        runs = argv.get(i + 1).ok_or("--runs needs a number")?.parse()?;
        argv.drain(i..=i + 1);
    }
    let positional: Vec<&String> = argv.iter().filter(|a| !a.starts_with("--")).collect();
    let modes: Vec<&str> = match positional.first() {
        Some(m) => vec![m.as_str()],
        None => vec!["loop", "plan", "parallel"],
    };
    let qs: Vec<usize> = match positional.get(1) {
        Some(q) => vec![q.parse()?],
        None => vec![1, 2, 3, 4],
    };

    println!("| question | mode | correct | LLM calls | input tokens | tool seconds | total seconds |");
    println!("|---|---|---|---|---|---|---|");
    for q in qs {
        let (question, expected) = QUESTIONS[q - 1];
        for mode in &modes {
            let (mut ok, mut calls, mut tokens, mut tool_s, mut total_s) = (0, vec![], vec![], vec![], vec![]);
            for _ in 0..runs {
                let t0 = Instant::now();
                let outcome = match *mode {
                    "loop" => Ok(mode_loop(&client, question, with_sum, verbose).await),
                    "plan" => mode_plan(&client, question, false, verbose).await,
                    _ => mode_plan(&client, question, true, verbose).await,
                };
                // The provider gave up after retries: a failed cell, not a crash.
                let (answer, c, t, ts) = outcome.unwrap_or_else(|e| (format!("provider error: {e}"), 0, 0, 0.0));
                if answer.replace(',', "").contains(expected) {
                    ok += 1;
                } else {
                    let short: String = answer.trim().replace('\n', " ").chars().take(140).collect();
                    println!("|   | wrong: {short} | | | | | |");
                }
                calls.push(c as f64);
                tokens.push(t as f64);
                tool_s.push(ts);
                total_s.push(t0.elapsed().as_secs_f64());
            }
            println!("| {q} | {mode} | {ok}/{runs} | {} | {:.0} | {:.1} | {:.1} |",
                     median(calls), median(tokens), median(tool_s), median(total_s));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, tool: ToolName, args: &[(&str, &str)]) -> Step {
        Step {
            id: id.into(),
            why: "test".into(),
            tool,
            args: args.iter().map(|(n, v)| Arg { name: n.to_string(), value: v.to_string() }).collect(),
        }
    }

    fn fast() {
        TOOL_LATENCY_MS.store(300, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn a_reference_is_replaced_by_the_earlier_result() {
        fast();
        let plan = Plan { steps: vec![
            step("s1", ToolName::NearestAirport, &[("town", "Vaslui")]),
            step("s2", ToolName::FlightPrice, &[("origin", "@s1"), ("destination", "Rome"), ("date", "12 May")]),
        ] };
        let r = execute(&plan, false).await;
        assert_eq!(r["s1"], "Iasi");
        assert!(r["s2"].contains("\"price_eur\":142"), "{}", r["s2"]);
    }

    #[tokio::test]
    async fn parallel_runs_independent_steps_at_the_same_time() {
        fast();
        let plan = Plan { steps: ["Rome", "Barcelona", "Lisbon"].iter().enumerate()
            .map(|(i, c)| step(&format!("s{}", i + 1), ToolName::Weather, &[("city", c)])).collect() };
        let t0 = Instant::now();
        execute(&plan, false).await;
        let sequential = t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        let r = execute(&plan, true).await;
        let parallel = t0.elapsed().as_secs_f64();
        assert!(sequential >= 0.85 && parallel < 0.6, "{sequential} {parallel}");
        assert_eq!(r["s3"], "21 C, windy");
    }

    #[tokio::test]
    async fn a_dependent_step_waits_for_its_input_even_in_parallel() {
        fast();
        let plan = Plan { steps: vec![
            step("s2", ToolName::FlightPrice, &[("origin", "@s1"), ("destination", "Barcelona"), ("date", "12 May")]),
            step("s1", ToolName::NearestAirport, &[("town", "Botosani")]),
        ] };
        assert!(execute(&plan, true).await["s2"].contains("\"price_eur\":156"));
    }

    #[tokio::test]
    async fn a_reference_to_a_missing_step_does_not_hang() {
        fast();
        let plan = Plan { steps: vec![step("s1", ToolName::Weather, &[("city", "@s9")])] };
        assert!(execute(&plan, true).await["s1"].contains("No weather data"));
    }

    #[tokio::test]
    async fn bad_arguments_become_an_error_result() {
        fast();
        let plan = Plan { steps: vec![step("s1", ToolName::HotelPrice, &[("city", "Rome"), ("nights", "three")])] };
        assert!(execute(&plan, false).await["s1"].starts_with("ERROR"));
    }
}

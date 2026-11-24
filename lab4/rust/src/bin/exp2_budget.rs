//! Lab 4 - Experiment 2: the budget question (STARTER).
//!   cargo run --bin exp2_budget              # the short prompt vs your prompt
//!   cargo run --bin exp2_budget -- --code    # bonus 2: the model writes the arithmetic, your code computes it
//!   cargo test --bin exp2_budget             # parser and calculator tests
use genai::Client;
use lab4_rs::llm::ask;
use regex::Regex;
use std::collections::HashMap;

const TRIP: &str = "Two friends fly from Iasi to Rome. Flights cost EUR 129 per person, each way. \
They share one hotel room at EUR 85 per night, arriving on 12 May and leaving on 16 May. \
The city tax is EUR 2.50 per person per night. Each of them buys a EUR 45 museum pass. \
Their total budget is EUR 1000. Does the trip fit the budget, and by how much?";
const EXPECTED: (&str, f64) = ("yes", 34.0); // 516 flights + 340 hotel + 20 tax + 90 passes = 966
const FORMAT: &str = "Answer: <yes|no>, <difference in EUR>";

/// '34', '34.00', '1,034' and the European '34,00' all become numbers.
fn number(text: &str) -> Option<f64> {
    let t = text.trim_end_matches('.');
    let european = Regex::new(r"^\d+,\d{1,2}$").unwrap();
    let t = if european.is_match(t) { t.replace(',', ".") } else { t.replace(',', "") };
    t.parse().ok()
}

fn parse(text: &str) -> Option<(String, f64)> {
    let re = Regex::new(r"(?i)Answer:\s*(yes|no)\s*,\s*(?:EUR|€)?\s*([\d.,]+)").unwrap();
    let cleaned = text.replace('*', ""); // models like **Answer:**
    let c = re.captures(&cleaned)?;
    Some((c[1].to_lowercase(), number(&c[2])?))
}

/// Exact-enough, safe arithmetic: numbers, + - * / and parentheses only (a tiny recursive-descent parser).
fn evaluate(expr: &str) -> Option<f64> {
    fn expr_(t: &[char], i: &mut usize) -> Option<f64> {
        let mut v = term(t, i)?;
        while *i < t.len() && (t[*i] == '+' || t[*i] == '-') {
            let op = t[*i];
            *i += 1;
            let r = term(t, i)?;
            v = if op == '+' { v + r } else { v - r };
        }
        Some(v)
    }
    fn term(t: &[char], i: &mut usize) -> Option<f64> {
        let mut v = factor(t, i)?;
        while *i < t.len() && (t[*i] == '*' || t[*i] == '/') {
            let op = t[*i];
            *i += 1;
            let r = factor(t, i)?;
            v = if op == '*' { v * r } else if r == 0.0 { return None } else { v / r };
        }
        Some(v)
    }
    fn factor(t: &[char], i: &mut usize) -> Option<f64> {
        if *i < t.len() && t[*i] == '(' {
            *i += 1;
            let v = expr_(t, i)?;
            if *i < t.len() && t[*i] == ')' {
                *i += 1;
                return Some(v);
            }
            return None;
        }
        let start = *i;
        while *i < t.len() && (t[*i].is_ascii_digit() || t[*i] == '.') {
            *i += 1;
        }
        t[start..*i].iter().collect::<String>().parse().ok()
    }
    let tokens: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).map(|c| if c == '×' { '*' } else { c }).collect();
    let mut i = 0;
    let v = expr_(&tokens, &mut i)?;
    (i == tokens.len()).then_some(v) // anything left over is not allowed
}

async fn run_code(client: &Client) -> Result<(), String> {
    // BONUS 2 TODO: a prompt where the model does NOT calculate. It writes ONE arithmetic expression for
    // the total cost and ends with the line "Total: <expression>"; evaluate() does the arithmetic.
    let prompt = format!("{TRIP}\nTODO");
    let total_re = Regex::new(r"Total:\s*(.+)").unwrap();
    let mut correct = 0;
    for _ in 0..5 {
        let r = ask(client, None, &prompt, Some(0.7)).await?;
        let text = r.text.replace("**", ""); // drop Markdown bold, keep the * of multiplication
        let total = total_re.captures(&text).and_then(|c| evaluate(c[1].trim().trim_matches('`')));
        let verdict = total.map(|t| (if t <= 1000.0 { "yes" } else { "no" }, ((1000.0 - t).abs() * 100.0).round() / 100.0));
        if verdict == Some(EXPECTED) {
            correct += 1;
        }
        println!("   {:>5} tok   total = {total:?}   -> {verdict:?}", r.out_tokens);
    }
    println!("== code computes: {correct}/5 correct");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let client = Client::new()?;
    if std::env::args().any(|a| a == "--code") {
        run_code(&client).await?;
        return Ok(());
    }
    // The short prompt: same output line, no room to work.
    let short = format!("{TRIP}\nReply with exactly one line and nothing else: {FORMAT}");
    // TODO: write a better prompt for the SAME question. Two changes only:
    //   1. give the model room to work before it answers;
    //   2. keep the same last line, so your code can parse it: {FORMAT}
    let better = format!("{TRIP}\nTODO");

    for (name, prompt) in [("short", short), ("better", better)] {
        let mut rows = Vec::new();
        for _ in 0..5 {
            let r = ask(&client, None, &prompt, Some(0.7)).await?;
            let last: String = r.text.trim().lines().last().unwrap_or("").chars().take(60).collect();
            rows.push((parse(&r.text), r.out_tokens, last));
        }
        let correct = rows.iter().filter(|(p, _, _)| p.as_ref().is_some_and(|(a, d)| a == EXPECTED.0 && *d == EXPECTED.1)).count();
        let avg = rows.iter().map(|(_, t, _)| *t as f64).sum::<f64>() / rows.len() as f64;
        println!("\n== {name}: {correct}/{} correct, {avg:.0} output tokens on average", rows.len());
        for (p, t, last) in &rows {
            println!("   {:<22} {t:>5} tok   last line: {last}", format!("{p:?}"));
        }
        // BONUS 1 TODO: five answers to the same question - which one do you trust?
        // Count the parsed answers (a HashMap<String, usize>) and print the most common one.
        let _votes: HashMap<String, usize> = HashMap::new();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_answer_line() {
        assert_eq!(parse("Answer: yes, 34"), Some(("yes".into(), 34.0)));
        assert_eq!(parse("**Answer:** yes, €34"), Some(("yes".into(), 34.0)));
        assert_eq!(parse("Answer: Yes, 34,00"), Some(("yes".into(), 34.0)));
        assert_eq!(parse("Answer: no, 1,034."), Some(("no".into(), 1034.0)));
        assert_eq!(parse("about 34"), None);
    }

    #[test]
    fn evaluates_only_arithmetic() {
        assert_eq!(evaluate("2*2*129 + 4*85 + 2*4*2.5 + 2*45"), Some(966.0));
        assert_eq!(evaluate("(1 + 2) * 3"), Some(9.0));
        assert_eq!(evaluate("rm -rf /"), None);
        assert_eq!(evaluate("1/0"), None);
    }
}

//! Lab 4 - Experiment 4: a day that actually works (STARTER).  Tests: cargo test --bin exp4_day
use genai::Client;
use lab4_rs::llm::ask_json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const DAY_RULES: &str = "Plan ONE day in Rome. Schedule all five items, each exactly once:
- Borghese Gallery: 2 h. Entry ONLY at 09:00, 11:00, 13:00 or 15:00.
- Colosseum: 2 h. Entry on the hour (xx:00), from 09:00 to 15:00.
- Pantheon: 1 h. Open 09:00-19:00 (must be finished by 19:00).
- Lunch: 1 h. Must START between 12:00 and 14:00.
- Trastevere food walk: 1 h 30 min. Starts ONLY at 17:00 or 18:00.
Moving between two consecutive items takes 30 minutes. The day starts at 09:00 and must end by 20:00.
Return the schedule in time order, times as HH:MM.";

const HINT: &str = "\nBefore writing the schedule, first make a plan: place the items with the fewest allowed start \
times first, then fit the others around them, and check every rule (including the 30-minute moves) before you answer.";

const NAMES: [&str; 5] = ["Borghese Gallery", "Colosseum", "Pantheon", "Lunch", "Trastevere food walk"];

/// (duration in minutes, allowed start times in minutes), or None for an unknown name.
fn rule(name: &str) -> Option<(u32, Vec<u32>)> {
    Some(match name {
        "Borghese Gallery" => (120, vec![540, 660, 780, 900]),
        "Colosseum" => (120, (9..=15).map(|h| h * 60).collect()),
        "Pantheon" => (60, (540..=1080).collect()),
        "Lunch" => (60, (720..=840).collect()),
        "Trastevere food walk" => (90, vec![1020, 1080]),
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Item {
    /// Exactly one of: Borghese Gallery, Colosseum, Pantheon, Lunch, Trastevere food walk
    // A free string on purpose: the verifier catches any other spelling as an unknown item.
    name: String,
    start: String,
    end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Schedule {
    items: Vec<Item>,
}

/// TODO (step 3): the same output, but give the model somewhere to plan BEFORE it writes the items.
/// Hint: the model writes the fields in the order you declare them.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ScheduleWithPlan {
    items: Vec<Item>,
}

fn minutes(hhmm: &str) -> Option<u32> {
    let (h, m) = hhmm.trim().split_once(':')?;
    Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
}

fn hhmm(m: u32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Deterministic verifier: returns the broken rules (empty = valid).
fn check(items: &[Item]) -> Vec<String> {
    // TODO (step 1): check every rule in DAY_RULES, using NAMES, rule(), minutes() and hhmm().
    // Make the tests pass. Use these exact messages (the tests check them):
    //   "{name} appears {count} times"                - each name in NAMES exactly once
    //   "unknown item '{name}'"                        - rule(name) is None
    //   "{name}: bad time format"                      - minutes() returned None
    //   "{name}: lasts {length} min, must be {duration}"
    //   "{name}: cannot start at HH:MM"                - start not in the allowed list
    //   "{first} -> {second}: needs 30 min to move"    - consecutive items, sorted by start
    //   "outside 09:00-20:00"
    let _ = (items, NAMES, rule(""), hhmm(0));
    vec!["TODO: verifier not implemented".into()]
}

/// TODO (step 2): keep only the valid samples (no errors) and return the schedule whose first item
/// starts latest (Ana hates early mornings), or None if no sample is valid.
fn best(samples: &[(Option<Vec<Item>>, Vec<String>)]) -> Option<&Vec<Item>> {
    let _ = samples;
    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let client = Client::new()?;
    let with_hint = format!("{DAY_RULES}{HINT}");
    let runs: [(&str, &str, bool); 3] = [
        ("plain", DAY_RULES, false),
        ("hint in the prompt, JSON only", &with_hint, false),
        ("hint + a field to plan in", &with_hint, true),
    ];
    for (label, prompt, with_plan) in runs {
        let mut samples: Vec<(Option<Vec<Item>>, Vec<String>)> = Vec::new();
        let mut tokens = 0;
        for _ in 0..5 {
            // Malformed JSON (or a failed call) is an invalid sample, not a crash.
            let items = if with_plan {
                ask_json::<ScheduleWithPlan>(&client, None, prompt, Some(1.0)).await.map(|(s, r)| (s.items, r.out_tokens))
            } else {
                ask_json::<Schedule>(&client, None, prompt, Some(1.0)).await.map(|(s, r)| (s.items, r.out_tokens))
            };
            match items {
                Ok((items, out)) => {
                    tokens += out;
                    let errs = check(&items);
                    samples.push((Some(items), errs));
                }
                Err(e) => samples.push((None, vec![format!("unparseable output ({e})")])),
            }
        }
        let valid = samples.iter().filter(|(_, e)| e.is_empty()).count();
        println!("\n== {label}: {valid}/5 valid, {} output tokens on average", tokens / 5);
        for (_, errs) in &samples {
            match errs.first() {
                None => println!("   ok"),
                Some(e) if errs.len() > 1 => println!("   {e} (+{} more)", errs.len() - 1),
                Some(e) => println!("   {e}"),
            }
        }
        if let Some(chosen) = best(&samples) {
            let mut sorted = chosen.clone();
            sorted.sort_by_key(|i| minutes(&i.start));
            let s: Vec<String> = sorted.iter().map(|i| format!("{} {}", i.start, i.name)).collect();
            println!("   chosen: {}", s.join(" | "));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: [(&str, &str, &str); 5] = [
        ("Borghese Gallery", "09:00", "11:00"),
        ("Lunch", "12:00", "13:00"),
        ("Colosseum", "14:00", "16:00"),
        ("Pantheon", "16:30", "17:30"),
        ("Trastevere food walk", "18:00", "19:30"),
    ];

    fn day(rows: &[(&str, &str, &str)]) -> Vec<Item> {
        rows.iter().map(|&(n, s, e)| Item { name: n.into(), start: s.into(), end: e.into() }).collect()
    }

    /// The valid day with one item changed.
    fn replace(name: &str, start: &str, end: &str) -> Vec<Item> {
        let rows: Vec<(&str, &str, &str)> = VALID.iter().map(|&(n, s, e)| if n == name { (n, start, end) } else { (n, s, e) }).collect();
        day(&rows)
    }

    fn has(errors: &[String], msg: &str) -> bool {
        errors.iter().any(|e| e == msg)
    }

    #[test]
    fn accepts_a_valid_day() {
        assert!(check(&day(&VALID)).is_empty(), "{:?}", check(&day(&VALID)));
    }

    #[test]
    fn rejects_a_missing_move() {
        let errors = check(&day(&[
            ("Colosseum", "09:00", "11:00"),
            ("Borghese Gallery", "11:00", "13:00"),
            ("Lunch", "13:30", "14:30"),
            ("Pantheon", "15:00", "16:00"),
            ("Trastevere food walk", "17:00", "18:30"),
        ]));
        assert_eq!(errors, vec!["Colosseum -> Borghese Gallery: needs 30 min to move"]);
    }

    #[test]
    fn rejects_a_start_off_the_hour() {
        assert!(has(&check(&replace("Colosseum", "14:30", "16:30")), "Colosseum: cannot start at 14:30"));
    }

    #[test]
    fn rejects_a_wrong_duration() {
        assert!(has(&check(&replace("Pantheon", "16:30", "18:00")), "Pantheon: lasts 90 min, must be 60"));
    }

    #[test]
    fn rejects_lunch_outside_its_window() {
        assert!(has(&check(&replace("Lunch", "14:30", "15:30")), "Lunch: cannot start at 14:30"));
    }

    #[test]
    fn rejects_the_food_walk_at_the_wrong_time() {
        assert!(has(&check(&replace("Trastevere food walk", "18:30", "20:00")), "Trastevere food walk: cannot start at 18:30"));
    }

    #[test]
    fn rejects_a_missing_and_a_duplicated_item() {
        let mut rows = VALID[..4].to_vec();
        rows.push(("Pantheon", "18:00", "19:00"));
        let errors = check(&day(&rows));
        assert!(has(&errors, "Trastevere food walk appears 0 times"), "{errors:?}");
        assert!(has(&errors, "Pantheon appears 2 times"), "{errors:?}");
    }

    #[test]
    fn rejects_an_unknown_spelling() {
        let rows: Vec<(&str, &str, &str)> = VALID.iter().map(|&(n, s, e)| (if n == "Colosseum" { "colosseum" } else { n }, s, e)).collect();
        let errors = check(&day(&rows));
        assert!(has(&errors, "unknown item 'colosseum'"), "{errors:?}");
        assert!(has(&errors, "Colosseum appears 0 times"), "{errors:?}");
    }

    #[test]
    fn rejects_a_bad_time_format() {
        assert!(has(&check(&replace("Lunch", "noon", "13:00")), "Lunch: bad time format"));
    }

    #[test]
    fn rejects_a_day_that_starts_too_early() {
        assert!(has(&check(&replace("Borghese Gallery", "08:00", "10:00")), "outside 09:00-20:00"));
    }

    #[test]
    fn best_prefers_the_latest_start() {
        let later = day(&[
            ("Pantheon", "09:30", "10:30"),
            ("Borghese Gallery", "11:00", "13:00"),
            ("Lunch", "13:30", "14:30"),
            ("Colosseum", "15:00", "17:00"),
            ("Trastevere food walk", "18:00", "19:30"),
        ]);
        assert!(check(&later).is_empty());
        let samples = vec![(Some(day(&VALID)), vec![]), (Some(later), vec![]), (None, vec!["unparseable output".to_string()])];
        assert_eq!(best(&samples).map(|d| d[0].start.clone()), Some("09:30".to_string()));
        assert!(best(&[(None, vec!["unparseable output".to_string()])]).is_none());
    }
}

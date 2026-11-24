Lab 4 starters
==============

python/  -> copy your agent_core.py from Lab 3 into this folder, then run from here:
            python exp1_knowledge.py      pytest -v
rust/    -> copy src/agent_core.rs from your Lab 3 crate into rust/src/, then:
            cargo build                    (start this during Part 0: the first build takes a few minutes)
            cargo run --bin exp1_knowledge cargo test

The .env with GEMINI_API_KEY from Lab 2 must be in this folder or one of its parents.

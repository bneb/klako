use std::process::Command;

pub struct TaskSpec {
    pub id: String,
    pub writes: Vec<String>,
    pub reads: Vec<String>,
}

pub struct FormalShadow;

impl FormalShadow {
    pub fn verify_parallel_tasks(tasks: &[TaskSpec]) -> Result<(), String> {
        if tasks.len() < 2 {
            return Ok(());
        }

        let mut smt_script = String::new();
        smt_script.push_str("(set-logic QF_S)\n");
        smt_script.push_str("(declare-const P String)\n");

        // We want to prove there is no overlap between what task i writes and what task j accesses (reads or writes)
        // If there exists a P that satisfies both, it's SAT -> collision.
        let mut constraints = Vec::new();
        
        for i in 0..tasks.len() {
            for j in (i + 1)..tasks.len() {
                let task_i = &tasks[i];
                let task_j = &tasks[j];

                // Task i accesses P
                let mut i_accesses = Vec::new();
                for w in &task_i.writes {
                    i_accesses.push(format!("(= P \"{w}\")"));
                }
                for r in &task_i.reads {
                    i_accesses.push(format!("(= P \"{r}\")"));
                }
                let i_or = if i_accesses.is_empty() {
                    "false".to_string() 
                } else {
                    format!("(or {})", i_accesses.join(" "))
                };

                // Task j writes to P
                let mut j_writes = Vec::new();
                for w in &task_j.writes {
                    j_writes.push(format!("(= P \"{w}\")"));
                }
                let j_w_or = if j_writes.is_empty() {
                    "false".to_string()
                } else {
                    format!("(or {})", j_writes.join(" "))
                };

                // Task i writes to P
                let mut i_writes = Vec::new();
                for w in &task_i.writes {
                    i_writes.push(format!("(= P \"{w}\")"));
                }
                let i_w_or = if i_writes.is_empty() {
                    "false".to_string()
                } else {
                    format!("(or {})", i_writes.join(" "))
                };

                // Task j accesses P
                let mut j_accesses = Vec::new();
                for w in &task_j.writes {
                    j_accesses.push(format!("(= P \"{w}\")"));
                }
                for r in &task_j.reads {
                    j_accesses.push(format!("(= P \"{r}\")"));
                }
                let j_or = if j_accesses.is_empty() {
                    "false".to_string()
                } else {
                    format!("(or {})", j_accesses.join(" "))
                };

                constraints.push(format!(
                    "(or (and {i_w_or} {j_or}) (and {j_w_or} {i_or}))"
                ));
            }
        }

        if constraints.is_empty() {
            return Ok(());
        }

        smt_script.push_str(&format!("(assert (or {}))\n", constraints.join(" ")));
        smt_script.push_str("(check-sat)\n");

        let z3_path = std::env::var("Z3_BIN").unwrap_or_else(|_| "z3".to_string());
        
        let mut child = Command::new(&z3_path)
            .arg("-in")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn Z3: {e}"))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(smt_script.as_bytes()).map_err(|e| e.to_string())?;
        }

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        let result = String::from_utf8_lossy(&output.stdout);

        if result.contains("sat") && !result.contains("unsat") {
            return Err(format!("b4mal Formal Shadow detected task collision via Z3. Script:\n{smt_script}"));
        }

        Ok(())
    }
}

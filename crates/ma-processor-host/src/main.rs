//! `ma-processor-host`: one child per job. Reads exactly one request frame from stdin, runs the
//! processor, writes zero or more progress frames and exactly one result frame to stdout as JSON
//! lines. Phase 0 hosts the scripted processor so the engine's isolation and stall tests can drive
//! an abort, a silent stall or a normal run; Phase 3 adds the real adapters behind the same frames.

use ma_processor::{
    ProgressFrame, RequestFrame, ResultFrame, Script, ScriptedProcessor, StagedDir,
};
use std::io::{BufRead, Write};

fn emit<T: serde::Serialize>(frame: &T) {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, frame).expect("frame serializes");
    out.write_all(b"\n").expect("stdout");
    out.flush().expect("stdout");
}

fn main() {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .expect("one request frame on stdin");
    let request: RequestFrame = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(_) => {
            emit(&ResultFrame::Failed {
                failure: ma_processor::Failure::InvalidInput {
                    reason: "malformed request frame".into(),
                },
                completed_items: 0,
            });
            std::process::exit(2);
        }
    };
    let mut processor = ScriptedProcessor::transcription(&["ja", "en"]);
    for script in &request.script {
        match script.split_once(':') {
            Some(("abort_at", n)) => {
                processor = processor.with(Script::AbortAt(n.parse().unwrap_or(0)))
            }
            Some(("fail_at", n)) => {
                processor = processor.with(Script::FailRetryableAt(n.parse().unwrap_or(0)))
            }
            _ if script == "silent" => processor = processor.with(Script::Silent),
            _ => {}
        }
    }
    let staged = StagedDir::adopt(std::path::Path::new(&request.staged_dir));
    let silent = processor.scripts.contains(&Script::Silent);
    let mut completed = 0;
    for ordinal in 0..request.work_items {
        if processor.scripts.contains(&Script::AbortAt(ordinal)) {
            std::process::abort();
        }
        if silent {
            // alive and silent: the supervisor's stall watch must catch this
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
        match ma_processor::Processor::run_item(&mut processor, ordinal, &staged) {
            Ok(_) => {
                completed += 1;
                emit(&ProgressFrame {
                    completed_items: completed,
                    total_items: request.work_items,
                });
            }
            Err(failure) => {
                emit(&ResultFrame::Failed {
                    failure,
                    completed_items: completed,
                });
                return;
            }
        }
    }
    emit(&ResultFrame::Succeeded {
        completed_items: completed,
        output_digest: format!("{:016x}", completed as u64 * 0x9e37_79b9),
    });
}

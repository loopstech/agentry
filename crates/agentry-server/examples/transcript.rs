//! Print what Agentry reads from a Claude Code transcript:
//!     cargo run -p agentry-server --example transcript -- <path.jsonl>

use agentry_server::transcript::Transcript;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: transcript <path.jsonl>");
    let t = Transcript::open(std::path::Path::new(&path)).expect("can't read transcript");
    println!("{:#?}", t.info);
    for s in t.recent(50) {
        println!(
            "{:>3} {:<6} {:<8} {:<7} {}",
            s.seq,
            format!("{:?}", s.kind),
            s.tool.unwrap_or_default(),
            s.status.map(|x| format!("{x:?}")).unwrap_or_default(),
            s.text
        );
    }
}

//! Write every document `read_docs` serves, whole, as DIR/<topic>.md — the
//! pages the site's llms.txt links to — and print name, source and what each
//! answers, one tab-separated line per topic.
//!
//!     cargo run -q -p parcad-host --example docs_pages -- DIR

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: docs_pages DIR"));
    std::fs::create_dir_all(&dir).expect("create the output directory");
    for (name, source, answers, text) in parcad_host::docs::every_topic() {
        let page = format!(
            "<!-- read_docs topic \"{name}\", generated from {source} -->\n\n{}\n",
            text.trim_end()
        );
        std::fs::write(dir.join(format!("{name}.md")), page).expect("write a topic");
        println!("{name}\t{source}\t{answers}");
    }
}

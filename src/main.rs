//! The `plied` command (FORMAT.md §11).

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::{Args, CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

use plied::access::Policy;
use plied::check::{self, Level, Problem};
use plied::config;
use plied::doc::{self, Map, Node};
use plied::error::{Fail, Res, EXIT_ERROR, EXIT_GATE, EXIT_USAGE};
use plied::fsx::{self, FileLock};
use plied::literature::{Library, Status};
use plied::messages::Messages;
use plied::records::{OwnKind, Record, Store};
use plied::repo::Repo;
use plied::review;
use plied::views;
use plied::writer::{Who, Writer, apply_assignments};
use plied::{errln, gitguard, hook, index, outln, query, repo};

#[derive(Parser)]
#[command(name = "plied", version = plied::VERSION, about = "A git-tracked work record for teams of agents and people.")]
struct Cli {
    /// run as if started in PATH
    #[arg(short = 'C', value_name = "PATH")]
    c: Option<PathBuf>,
    /// use this plied.toml instead of the canonical checkout's
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    /// print one JSON result envelope on stdout, success or failure (FORMAT.md §11)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct WhoArgs {
    /// who is acting (default $PLIED_AGENT)
    #[arg(long = "as", value_name = "AGENT")]
    as_: Option<String>,
    /// session id (default $PLIED_SESSION or $CLAUDE_CODE_SESSION_ID)
    #[arg(long)]
    session: Option<String>,
}

#[derive(Subcommand)]
enum Cmd {
    /// write a plied.toml template here
    Init,
    /// print the agent skill for this version (needs no repository)
    Skill,
    /// show a record with its revision, links, backlinks, commitments and presence
    Show { id: String },
    /// create a record
    New {
        #[arg(value_parser = ["thread", "commitment", "question"])]
        kind: String,
        #[arg(long)]
        title: String,
        assign: Vec<String>,
        /// take a long value from a file (- for stdin)
        #[arg(long = "set-file", value_name = "key=FILE")]
        set_file: Vec<String>,
        /// initial Markdown body
        #[arg(long, value_name = "FILE")]
        body: Option<PathBuf>,
        #[arg(long)]
        why: Option<String>,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// change fields: key=value, key+=item, key-=item, key= (clear); dotted keys nest
    Set {
        id: String,
        assign: Vec<String>,
        #[arg(long = "set-file", value_name = "key=FILE")]
        set_file: Vec<String>,
        /// the revision you read (from `plied show`)
        #[arg(long, value_name = "REV")]
        expect: String,
        #[arg(long)]
        why: Option<String>,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// add a history line
    Note {
        id: String,
        text: String,
        #[arg(long, value_name = "REV")]
        expect: String,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// replace a record's whole file, checked like any write
    Replace {
        id: String,
        /// the new content (- for stdin)
        file: String,
        #[arg(long, value_name = "REV")]
        expect: String,
        #[arg(long)]
        why: Option<String>,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// list plied's records
    List {
        #[arg(value_parser = ["thread", "commitment", "question"])]
        kind: Option<String>,
        /// comma-separated states
        #[arg(long)]
        state: Option<String>,
    },
    /// filter all records with a JSON query
    Find { query: String },
    /// what cites or mentions a record
    Backlinks { id: String },
    /// print the work tree
    Tree { id: Option<String> },
    /// scheduled work by actor, unscheduled open threads, open questions and dates
    Frontier { #[arg(long)] actor: Option<String> },
    /// regenerate the Markdown view
    View {
        /// fail if the view is out of date
        #[arg(long)]
        check: bool,
        /// print it instead of writing
        #[arg(long)]
        print: bool,
    },
    /// finish or undo a write that a crash interrupted, and change nothing else
    Recover,
    /// remove cached revision content past runtime.revision_days (record files are untouched)
    Gc {
        /// remove all cached content, whatever its age
        #[arg(long)]
        all: bool,
    },
    /// validate everything (errors fail; warnings are for review)
    Check {
        #[arg(long = "errors-only")]
        errors_only: bool,
    },
    /// say you are on a node (an expiring lease)
    Claim {
        node: String,
        /// lease minutes (default from plied.toml)
        #[arg(long, value_name = "MIN")]
        ttl: Option<i64>,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// drop your claim(s)
    Release {
        node: Option<String>,
        /// only this session's claims
        #[arg(long = "this-session")]
        this_session: bool,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// live claims
    Who { node: Option<String> },
    /// message whoever is on a node, plus --to
    Send {
        node: String,
        /// the message (- for stdin)
        text: String,
        #[arg(long = "to", value_name = "AGENT")]
        to: Vec<String>,
        #[arg(long = "reply-to", value_name = "MSG")]
        reply_to: Option<String>,
        /// your own unique id, so a retry is recorded once
        #[arg(long, value_name = "ID")]
        id: Option<String>,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// receive every message on a node
    Subscribe { node: String, #[command(flatten)] who: WhoArgs },
    /// stop receiving a node's messages
    Unsubscribe { node: String, #[command(flatten)] who: WhoArgs },
    /// your messages (marks them read unless --peek)
    Inbox {
        /// include acknowledged ones
        #[arg(long)]
        all: bool,
        #[arg(long)]
        peek: bool,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// acknowledge receipt (not approval)
    Ack { ids: Vec<String>, #[command(flatten)] who: WhoArgs },
    /// a node's conversation
    Messages { node: String },
    /// agent hook entry point (reads the hook's JSON on stdin)
    Hook {
        #[arg(value_parser = ["claude"])]
        agent: String,
        #[arg(long)]
        debug: bool,
    },
    /// cleared literature
    Lit {
        #[command(subcommand)]
        lit_cmd: LitCmd,
    },
    /// build the search index (records and cleared paper pages)
    Index,
    /// keyword search over allowed records and cleared paper pages
    Search {
        terms: Vec<String>,
        #[arg(long, value_parser = ["record", "page"])]
        kind: Option<String>,
        #[arg(long)]
        paper: Option<String>,
        #[arg(short = 'n', default_value_t = 10)]
        n: usize,
    },
    /// commitments that apply to nodes before an action
    Review {
        nodes: Vec<String>,
        /// the action's name, e.g. run:fit-20261024
        #[arg(long)]
        action: Option<String>,
        /// exit 6 unless every applicable commitment has a non-blocking disposition for --action
        #[arg(long)]
        gate: bool,
    },
    /// record a commitment's disposition for an action
    Dispose {
        id: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        disposition: String,
        #[arg(long)]
        because: String,
        #[arg(long)]
        ref_: Option<String>,
        #[arg(long, value_name = "REV")]
        expect: String,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// record on a thread that a scanned linked record holds no commitment to record
    Scanned {
        /// the linked record you read
        id: String,
        /// its revision, as you read it (from `plied show`)
        #[arg(long, value_name = "REV")]
        rev: String,
        /// the thread that keeps the review
        #[arg(long, value_name = "THREAD")]
        on: String,
        #[arg(long)]
        because: String,
        /// the thread's revision you read
        #[arg(long, value_name = "REV")]
        expect: String,
        #[command(flatten)]
        who: WhoArgs,
    },
    /// git guards for record files
    Guard {
        #[command(subcommand)]
        guard_cmd: GuardCmd,
    },
}

#[derive(Subcommand)]
enum LitCmd {
    /// cleared papers
    List,
    /// a page's text, or --image for its rendering
    Page { paper: String, page: i64, #[arg(long)] image: bool },
    /// extract page text from the PDF
    Extract { paper: String },
    /// records that cite or mention a paper
    Cites { paper: String },
    /// every citation: clearance, hash, page, quotation
    Check,
    /// search cleared paper pages
    Search { terms: Vec<String>, #[arg(long)] paper: Option<String>, #[arg(short = 'n', default_value_t = 10)] n: usize },
}

#[derive(Subcommand)]
enum GuardCmd {
    PreCommit,
    PreMergeCommit,
    /// fail if HEAD brings record changes not on BASE
    Range { base: String, #[arg(default_value = "HEAD")] head: String },
    /// install the pre-commit and pre-merge-commit hooks
    Install { #[arg(long)] force: bool },
}

// --------------------------------------------------------------------------- context

struct Ctx {
    repo: Repo,
    lock: FileLock,
    policy: Rc<Policy>,
}

impl Ctx {
    fn new(cli: &Cli) -> Res<Ctx> {
        let repo = repo::locate(cli.c.as_deref(), cli.config.as_deref())?;
        let policy = Rc::new(Policy::new(&repo.cfg)?);
        let lock = FileLock::new(repo.runtime.join("lock"), 15.0);
        Ok(Ctx { repo, lock, policy })
    }

    fn writer(&self) -> Writer<'_> {
        Writer::new(&self.repo, &self.lock)
    }

    /// The records as read now. Reads take no lock, so a write in progress or interrupted can show
    /// some files replaced and others not; say so when an intent is seen before or after the read.
    fn store(&self) -> Res<Store> {
        let pending = || fsx::pending_intent(&self.repo.runtime).unwrap_or(true);
        let before = pending();
        if let Some((_, msg)) = self.repo.cfg.compat_problems().into_iter().find(|(s, _)| *s == "format") {
            plied::notes::note(format!("{msg}; what follows may misread it"));
        }
        let st = Store::load(&self.repo.cfg, None, &self.policy)?;
        if before || pending() {
            plied::notes::note("a write is in progress or was interrupted, so this read may mix old and new files (`plied recover` finishes an interrupted one)");
        }
        Ok(st)
    }

    fn messages(&self) -> Messages<'_> {
        Messages::new(&self.repo.cfg, &self.policy, &self.repo.runtime, &self.lock)
    }

    fn library(&self) -> Res<Library<'_>> {
        Library::new(&self.repo.cfg, &self.policy, &self.repo.runtime)
    }

    /// The library when literature is configured (errors propagate).
    fn library_opt(&self) -> Res<Option<Library<'_>>> {
        if self.repo.cfg.literature.is_none() {
            return Ok(None);
        }
        Ok(Some(self.library()?))
    }
}

fn who_of(args: &WhoArgs, required: bool) -> Res<Who> {
    let agent = args
        .as_
        .clone()
        .or_else(|| std::env::var("PLIED_AGENT").ok())
        .filter(|s| !s.is_empty());
    let session = args
        .session
        .clone()
        .or_else(|| std::env::var("PLIED_SESSION").ok())
        .or_else(|| std::env::var("CLAUDE_CODE_SESSION_ID").ok())
        .filter(|s| !s.is_empty());
    match agent {
        Some(agent) => Ok(Who { agent, session }),
        None if required => Err(Fail::code(EXIT_USAGE, "who is this? pass --as NAME or set PLIED_AGENT")),
        None => Ok(Who { agent: String::new(), session }),
    }
}

thread_local! {
    /// the command's result, held for the `--json` envelope
    static RESULT: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

/// Set the command's JSON result; `real_main` prints it inside the envelope (FORMAT.md §11).
fn out_json(v: &Value) {
    RESULT.with_borrow_mut(|r| *r = Some(v.clone()));
}

/// The `--json` envelope: one object on stdout, whatever happened.
fn envelope(code: i32, err: Option<&Fail>) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("plied".into(), json!(1));
    o.insert("ok".into(), json!(code == 0));
    o.insert("exit".into(), json!(code));
    o.insert("status".into(), json!(plied::error::exit_name(code)));
    match err {
        Some(e) => {
            let mut eo = serde_json::Map::new();
            eo.insert("kind".into(), json!(e.kind.name()));
            eo.insert("message".into(), json!(e.msg));
            if let Some(d) = &e.detail {
                eo.insert("detail".into(), d.clone());
            }
            o.insert("error".into(), Value::Object(eo));
        }
        None => {
            o.insert("result".into(), RESULT.with_borrow_mut(Option::take).unwrap_or(Value::Null));
        }
    }
    o.insert("notes".into(), json!(plied::notes::take()));
    Value::Object(o)
}

fn read_stdin() -> Res<String> {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| Fail::new(e.to_string()))?;
    Ok(s)
}

fn assignments_of(assign: &[String], set_file: &[String]) -> Res<Vec<String>> {
    let mut out: Vec<String> = assign.to_vec();
    for item in set_file {
        let (key, path) = match item.split_once('=') {
            Some((k, p)) => (k, p),
            None => return Err(Fail::new(format!("--set-file expects key=FILE, got {item:?}"))),
        };
        let text = if path == "-" { read_stdin()? } else { fsx::read_text(Path::new(path))? };
        let text = text.strip_suffix('\n').unwrap_or(&text);
        out.push(format!("{key}={}", serde_json::to_string(text).unwrap_or_default()));
    }
    Ok(out)
}

/// The agent skill, rendered from this binary's own version and command inventory.
fn skill_text() -> String {
    fn walk(cmd: &clap::Command, prefix: &str, lines: &mut Vec<String>) {
        for sub in cmd.get_subcommands() {
            if sub.is_hide_set() {
                continue;
            }
            let name = format!("{prefix} {}", sub.get_name());
            let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
            lines.push(if about.is_empty() { format!("- `{name}`") } else { format!("- `{name}` — {about}") });
            walk(sub, &name, lines);
        }
    }
    let mut lines = vec![];
    walk(&Cli::command(), "plied", &mut lines);
    plied::skill::render(plied::VERSION, &lines)
}

fn main() {
    let code = real_main();
    if code != 0 {
        std::process::exit(code);
    }
}

fn real_main() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            // help and version are not failures; a usage error under --json still gets its envelope
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand)
                || !std::env::args().any(|a| a == "--json")
            {
                e.exit();
            }
            let f = Fail::code(EXIT_USAGE, e.render().to_string().trim().to_string());
            outln!("{}", serde_json::to_string_pretty(&envelope(EXIT_USAGE, Some(&f))).unwrap_or_default());
            return EXIT_USAGE;
        }
    };
    if cli.json && !matches!(cli.cmd, Cmd::Hook { .. }) {
        plied::notes::collect();
        let (code, err) = match run(&cli) {
            Ok(code) => (code, None),
            Err(e) => (e.code, Some(e)),
        };
        outln!("{}", serde_json::to_string_pretty(&envelope(code, err.as_ref())).unwrap_or_default());
        return code;
    }
    match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            errln!("plied: {}", e.msg);
            e.code
        }
    }
}

fn run(cli: &Cli) -> Res<i32> {
    match &cli.cmd {
        Cmd::Init => cmd_init(cli),
        // no repository: the skill describes this binary, wherever it runs
        Cmd::Skill => {
            let text = skill_text();
            if cli.json {
                out_json(&json!({"version": plied::VERSION, "skill": text}));
            } else {
                outln!("{text}");
            }
            Ok(0)
        }
        Cmd::Hook { agent, debug } => {
            if agent != "claude" {
                return Err(Fail::code(EXIT_USAGE, "only `plied hook claude` exists so far; other agents poll with `plied inbox`"));
            }
            Ok(hook::main_claude(*debug))
        }
        Cmd::Guard { guard_cmd } => cmd_guard(cli, guard_cmd),
        _ => {
            let ctx = Ctx::new(cli)?;
            dispatch(&ctx, cli)
        }
    }
}

fn dispatch(ctx: &Ctx, cli: &Cli) -> Res<i32> {
    let json = cli.json;
    match &cli.cmd {
        Cmd::Show { id } => cmd_show(ctx, id, json),
        Cmd::New { kind, title, assign, set_file, body, why, who } => {
            let kind = OwnKind::parse(kind).unwrap();
            let who = who_of(who, true)?;
            let mut meta = Map::new();
            meta.insert("title".into(), Node::Str(title.clone()));
            let def = if kind == OwnKind::Thread { ("status", "open") } else { ("state", "open") };
            meta.insert(def.0.into(), Node::Str(def.1.into()));
            let assigns = assignments_of(assign, set_file)?;
            apply_assignments(&mut meta, &assigns)?;
            let body_text = match body {
                Some(p) => Some(fsx::read_text(p)?),
                None => None,
            };
            let lib = ctx.library_opt()?;
            let (rid, rev) = ctx.writer().create(&ctx.policy, lib.as_ref(), kind, meta, &who, why.as_deref(), body_text)?;
            if json {
                out_json(&json!({"id": rid, "rev": rev}));
            } else {
                outln!("created {rid} (rev {})", &rev[..12.min(rev.len())]);
            }
            Ok(0)
        }
        Cmd::Set { id, assign, set_file, expect, why, who } => {
            let who = who_of(who, true)?;
            let assigns = assignments_of(assign, set_file)?;
            let lib = ctx.library_opt()?;
            let rev = ctx.writer().update(&ctx.policy, lib.as_ref(), id, expect, &who, why.as_deref(), &assigns, None, None, None)?;
            print_updated(id, &rev, json, "updated");
            Ok(0)
        }
        Cmd::Note { id, text, expect, who } => {
            let who = who_of(who, true)?;
            let lib = ctx.library_opt()?;
            let rev = ctx.writer().update(&ctx.policy, lib.as_ref(), id, expect, &who, None, &[], Some(text), None, None)?;
            print_updated(id, &rev, json, "noted");
            Ok(0)
        }
        Cmd::Replace { id, file, expect, why, who } => {
            let who = who_of(who, true)?;
            let text = if file == "-" { read_stdin()? } else { fsx::read_text(Path::new(file))? };
            let lib = ctx.library_opt()?;
            let rev = ctx.writer().update(&ctx.policy, lib.as_ref(), id, expect, &who, why.as_deref(), &[], None, Some(&text), None)?;
            print_updated(id, &rev, json, "replaced");
            Ok(0)
        }
        Cmd::List { kind, state } => cmd_list(ctx, kind.as_deref(), state.as_deref(), json),
        Cmd::Find { query: q } => cmd_find(ctx, q, json),
        Cmd::Backlinks { id } => {
            let st = ctx.store()?;
            let out = backlinks(&st, id);
            if json {
                out_json(&Value::Array(out.iter().map(|(i, h)| json!({"id": i, "how": h})).collect()));
            } else {
                for (i, h) in &out {
                    let t = st.get(i);
                    outln!("{i:6} {:22} {}", h.join(", "), t.map(|r| doc::take(&r.title(), 90).to_string()).unwrap_or_default());
                }
            }
            Ok(0)
        }
        Cmd::Tree { id } => {
            let st = ctx.store()?;
            if let Some(id) = id {
                let r = st.get(id).ok_or_else(|| Fail::new(format!("no record {id}")))?;
                if r.own() != Some(OwnKind::Thread) {
                    return Err(Fail::new(format!("{} is a {}, not a thread", r.id, r.kind.name())));
                }
            }
            if json {
                out_json(&views::tree_json(&st, id.as_deref()));
            } else {
                outln!("{}", views::tree_lines(&st, id.as_deref(), true).join("\n"));
            }
            Ok(0)
        }
        Cmd::Frontier { actor } => cmd_frontier(ctx, actor.as_deref(), json),
        Cmd::View { check, print } => cmd_view(ctx, *check, *print, json),
        Cmd::Check { errors_only } => cmd_check(ctx, *errors_only, json),
        Cmd::Recover => cmd_recover(ctx, json),
        Cmd::Gc { all } => {
            let n = ctx.writer().prune(*all)?;
            if json {
                out_json(&json!({"removed": n}));
            } else {
                outln!("removed {n} cached revision(s)");
            }
            Ok(0)
        }
        Cmd::Claim { node, ttl, who } => {
            let who = who_of(who, true)?;
            let st = ctx.store()?;
            let r = st.get(node).ok_or_else(|| Fail::new(format!("no record {node}")))?;
            let c = ctx.messages().claim(node, &who.agent, who.session.as_deref(), *ttl)?;
            // whoever takes up a node sees what it obliges before acting on it (§12)
            let apps = node_commitments(&st, r);
            if json {
                let mut o = c.to_json();
                o.as_object_mut().unwrap().insert("commitments".into(), commitments_json(&apps));
                out_json(&o);
            } else {
                outln!("{} is on {node} until {} UTC (renewed by the hook, or claim again)", who.agent, c.expires().format("%Y-%m-%d %H:%M"));
                if !apps.is_empty() {
                    outln!("Commitments that apply (review before acting: `plied review {node}`):");
                    outln!("{}", commitment_lines(&apps).join("\n"));
                }
            }
            Ok(0)
        }
        Cmd::Release { node, this_session, who } => {
            let who = who_of(who, true)?;
            let n = ctx.messages().release(node.as_deref(), &who.agent, if *this_session { who.session.as_deref() } else { None })?;
            if json {
                out_json(&json!({"released": n}));
            } else {
                outln!("released {n} claim(s)");
            }
            Ok(0)
        }
        Cmd::Who { node } => {
            let cs: Vec<_> = ctx.messages().claims(false).into_iter().filter(|c| node.as_deref().is_none_or(|n| c.node == n)).collect();
            if json {
                out_json(&Value::Array(
                    cs.iter()
                        .map(|c| {
                            let mut o = c.to_json();
                            o.as_object_mut().unwrap().insert("expires".into(), json!(c.expires().to_rfc3339()));
                            o
                        })
                        .collect(),
                ));
            } else {
                for c in &cs {
                    let s = c.session.as_ref().map(|s| format!(" session {}", doc::take(s, 8))).unwrap_or_default();
                    outln!("{:6} {}{s}, since {}, lease until {} UTC", c.node, c.agent, c.since, c.expires().format("%H:%M"));
                }
                if cs.is_empty() {
                    outln!("nobody has a live claim");
                }
            }
            Ok(0)
        }
        Cmd::Send { node, text, to, reply_to, id, who } => {
            let who = who_of(who, true)?;
            if ctx.store()?.get(node).is_none() {
                return Err(Fail::new(format!("no record {node}")));
            }
            let text = if text == "-" { read_stdin()? } else { text.clone() };
            let (e, fresh) = ctx.messages().send(node, &text, &who.agent, who.session.as_deref(), to, reply_to.as_deref(), id.as_deref())?;
            if json {
                out_json(&json!({"event": e, "recorded_now": fresh}));
            } else {
                let status = if fresh { "sent" } else { "already recorded (retry with the same id)" };
                let to_s = e.get("to").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                let eid = e.get("id").and_then(Value::as_str).unwrap_or_default();
                outln!("{status}: {eid} on {node} to {}", if to_s.is_empty() { "nobody (no live claims or subscribers)".into() } else { to_s });
            }
            Ok(0)
        }
        Cmd::Subscribe { node, who } => subscribe(ctx, node, who, true, json),
        Cmd::Unsubscribe { node, who } => subscribe(ctx, node, who, false, json),
        Cmd::Inbox { all, peek, who } => cmd_inbox(ctx, *all, *peek, json, who),
        Cmd::Ack { ids, who } => {
            if ids.is_empty() {
                return Err(Fail::code(EXIT_USAGE, "ack needs at least one message id"));
            }
            let who = who_of(who, true)?;
            let n = ctx.messages().mark("ack", ids, &who.agent, who.session.as_deref(), None)?;
            if json {
                out_json(&json!({"acknowledged": n}));
            } else {
                outln!("acknowledged {n} message(s) (receipt only, not approval)");
            }
            Ok(0)
        }
        Cmd::Messages { node } => cmd_messages(ctx, node, json),
        Cmd::Lit { lit_cmd } => cmd_lit(ctx, lit_cmd, json),
        Cmd::Index => {
            let st = ctx.store()?;
            let ix = index::build(&ctx.repo.cfg, &ctx.policy, &ctx.repo.runtime, Some(&st))?;
            let docs = ix["docs"].as_array().cloned().unwrap_or_default();
            let pages = docs.iter().filter(|d| d["kind"].as_str() == Some("page")).count();
            if json {
                out_json(&json!({"records": docs.len() - pages, "pages": pages, "policy": ix["policy"]}));
            } else {
                outln!(
                    "indexed {} records and {pages} paper pages (policy {})",
                    docs.len() - pages,
                    doc::take(ix["policy"].as_str().unwrap_or_default(), 12)
                );
            }
            Ok(0)
        }
        Cmd::Search { terms, kind, paper, n } => cmd_search(ctx, &terms.join(" "), *n, kind.as_deref(), paper.as_deref(), json),
        Cmd::Review { nodes, action, gate } => cmd_review(ctx, nodes, action.as_deref(), *gate, json),
        Cmd::Dispose { id, action, disposition, because, ref_, expect, who } => {
            cmd_dispose(ctx, id, action, disposition, because, ref_.as_deref(), expect, who, json)
        }
        Cmd::Scanned { id, rev, on, because, expect, who } => cmd_scanned(ctx, id, rev, on, because, expect, who, json),
        _ => unreachable!(),
    }
}

fn print_updated(id: &str, rev: &str, json: bool, verb: &str) {
    if json {
        out_json(&json!({"id": id, "rev": rev}));
    } else {
        outln!("{id} {verb} (rev {})", &rev[..12.min(rev.len())]);
    }
}

fn subscribe(ctx: &Ctx, node: &str, who_args: &WhoArgs, on: bool, json: bool) -> Res<i32> {
    let who = who_of(who_args, true)?;
    if ctx.store()?.get(node).is_none() {
        return Err(Fail::new(format!("no record {node}")));
    }
    let mut m = serde_json::Map::new();
    m.insert("v".into(), json!(1));
    m.insert("id".into(), json!(plied::messages::ulid()));
    m.insert("type".into(), json!(if on { "subscribe" } else { "unsubscribe" }));
    m.insert("node".into(), json!(node));
    m.insert("agent".into(), json!(who.agent));
    m.insert("at".into(), json!(plied::messages::iso()));
    ctx.messages().append(node, vec![m.clone()])?;
    if json {
        out_json(&json!({"event": m}));
    } else {
        outln!("{} {} {node}", who.agent, if on { "subscribed to" } else { "unsubscribed from" });
    }
    Ok(0)
}

// --------------------------------------------------------------------------- commands

fn cmd_init(cli: &Cli) -> Res<i32> {
    let start = cli.c.clone().unwrap_or_else(|| PathBuf::from("."));
    let top = repo::git(&start, &["rev-parse", "--show-toplevel"]);
    let base = top.map(PathBuf::from).unwrap_or_else(|| fsx::resolve(&start));
    let p = base.join(config::CONFIG_NAME);
    if p.exists() {
        return Err(Fail::new(format!("{} already exists", p.display())));
    }
    let text = config::TEMPLATE.replace("{version}", plied::VERSION);
    std::fs::write(&p, text)?;
    if cli.json {
        out_json(&json!({"path": p.display().to_string()}));
    } else {
        outln!("wrote {}", p.display());
    }
    // outside git, keep the runtime directory out of version control
    let gi = base.join(".gitignore");
    let line = ".plied-runtime/";
    let is_git = repo::git(&start, &["rev-parse", "--show-toplevel"]).is_some();
    if !is_git {
        let cur = std::fs::read_to_string(&gi).unwrap_or_default();
        if !cur.lines().any(|l| l.trim() == line) {
            let mut fh = std::fs::OpenOptions::new().create(true).append(true).open(&gi)?;
            use std::io::Write as _;
            writeln!(fh, "\n{line}")?;
        }
    }
    Ok(0)
}

fn cmd_guard(cli: &Cli, guard_cmd: &GuardCmd) -> Res<i32> {
    let repo = repo::locate(cli.c.as_deref(), cli.config.as_deref())?;
    if repo.common_dir.is_none() {
        return Err(Fail::new("not a git repository"));
    }
    let lock = FileLock::new(repo.runtime.join("lock"), 15.0);
    let writer = Writer::new(&repo, &lock);
    let written = writer.written_revs();
    // a pending intent means some managed files may be the new revision and others the old, and the
    // new revisions are already listed as written: nothing staged can be vouched for until it is resolved
    let pending = || match fsx::pending_intent(&repo.runtime) {
        Ok(false) => None,
        Ok(true) => Some("a plied write is in progress or was interrupted; run `plied recover` first".to_string()),
        Err(e) => Some(format!("cannot tell whether a plied write is pending: {}", e.msg)),
    };
    let result: Result<Vec<String>, String> = match guard_cmd {
        GuardCmd::PreCommit => pending().map_or_else(|| gitguard::check_staged(&repo, &written, false), |p| Ok(vec![p])),
        GuardCmd::PreMergeCommit => pending().map_or_else(|| gitguard::check_merge(&repo, &written), |p| Ok(vec![p])),
        GuardCmd::Range { base, head } => gitguard::check_range(&repo, base, head),
        GuardCmd::Install { force } => {
            let lines = gitguard::install(&repo, *force).map_err(Fail::new)?;
            if cli.json {
                out_json(&json!({"installed": lines}));
            } else {
                for line in &lines {
                    outln!("{line}");
                }
            }
            return Ok(0);
        }
    };
    let problems = match result {
        Ok(p) => p,
        Err(e) => vec![format!("cannot check, so blocking: {e}")],
    };
    if cli.json {
        out_json(&json!({"problems": problems}));
    } else {
        for p in &problems {
            errln!("plied guard: {p}");
        }
        if !problems.is_empty() {
            errln!("plied guard: record files change only through `plied` in the canonical checkout. (Passing this guard never authorises a commit.)");
        }
    }
    Ok(if problems.is_empty() { 0 } else { plied::error::EXIT_BLOCKED })
}

/// Records that point at `rid`, with how: the fields that reference it, or `mentions`.
fn backlinks(st: &Store, rid: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    for r in &st.records {
        if r.id == rid {
            continue;
        }
        let mut hows: Vec<String> = if r.is_own() {
            let scanned = r.scanned().into_iter().filter(|(x, _)| x == rid).map(|_| "scanned".to_string());
            r.refs().into_iter().filter(|(_, x)| x == rid).map(|(f, _)| f.to_string()).chain(scanned).collect()
        } else {
            vec![]
        };
        if hows.is_empty() && st.mentions(&r.text).contains(rid) {
            hows = vec!["mentions".into()];
        }
        if !hows.is_empty() {
            hows.sort();
            hows.dedup();
            out.push((r.id.clone(), hows));
        }
    }
    out.sort();
    out
}

fn cmd_show(ctx: &Ctx, id: &str, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let r = st.get(id).ok_or_else(|| Fail::new(format!("no record {id}")))?;
    let data = std::fs::read(&r.path).unwrap_or_default();
    let _ = ctx.writer().remember(&data, false);
    if json {
        let mut o = r.to_json(&ctx.repo.cfg.base);
        o.insert("backlinks".into(), Value::Array(backlinks(&st, &r.id).iter().map(|(i, h)| json!({"id": i, "how": h})).collect()));
        if takes_commitments(r) {
            o.insert("commitments".into(), commitments_json(&node_commitments(&st, r)));
        }
        out_json(&Value::Object(o));
        return Ok(0);
    }
    outln!("{}", record_text(ctx, &st, r)?);
    Ok(0)
}

fn record_text(ctx: &Ctx, st: &Store, r: &Record) -> Res<String> {
    let mut l = vec![
        format!("{}  {}", r.id, r.title()),
        format!("kind: {}   rev: {}   file: {}", r.kind.name(), &r.rev[..12.min(r.rev.len())], fsx::rel(&ctx.repo.cfg.base, &r.path)),
        String::new(),
        r.text.trim_end().to_string(),
    ];
    if r.is_own() {
        let refs = r.refs();
        if !refs.is_empty() {
            l.push(String::new());
            l.push("Links:".into());
            for (f, rid) in refs {
                let t = st.get(&rid);
                l.push(format!("  {f}: {rid} — {}", t.map(|t| doc::take(&t.title(), 90).to_string()).unwrap_or_else(|| "(missing)".into())));
            }
        }
    }
    let back = backlinks(st, &r.id);
    if !back.is_empty() {
        l.push(String::new());
        l.push("Cited by:".into());
        for (b, how) in back {
            l.push(format!("  {b} ({})", how.join(", ")));
        }
    }
    if takes_commitments(r) {
        let apps = node_commitments(st, r);
        if !apps.is_empty() {
            l.push(String::new());
            l.push("Commitments that apply:".into());
            l.extend(commitment_lines(&apps));
        }
        let msgs = ctx.messages().events(&r.id);
        let n = msgs.iter().filter(|e| e.get("type").and_then(Value::as_str) == Some("message")).count();
        if n > 0 {
            l.push(String::new());
            l.push(format!("Messages: {n} (`plied messages {}`)", r.id));
        }
        let claims: Vec<_> = ctx.messages().claims(false).into_iter().filter(|c| c.node == r.id).collect();
        if !claims.is_empty() {
            l.push(format!("On it now (leases): {}", claims.iter().map(|c| format!("{} until {} UTC", c.agent, c.expires().format("%H:%M"))).collect::<Vec<_>>().join(", ")));
        }
    }
    Ok(l.join("\n"))
}

/// Threads and questions are the nodes `show` and `claim` list applicable commitments for.
fn takes_commitments(r: &Record) -> bool {
    r.is_own() && matches!(r.own(), Some(OwnKind::Thread | OwnKind::Question))
}

fn node_commitments<'s>(st: &'s Store, r: &Record) -> Vec<review::Applicable<'s>> {
    if takes_commitments(r) { review::applicable(st, std::slice::from_ref(&r.id), None) } else { vec![] }
}

fn commitments_json(apps: &[review::Applicable]) -> Value {
    Value::Array(
        apps.iter()
            .map(|a| json!({"id": a.commitment.id, "rev": a.commitment.rev, "state": a.commitment.state_str(), "title": a.commitment.title(), "chains": a.chains, "governs": governs_json(a.commitment)}))
            .collect(),
    )
}

/// A commitment's `governs` list; None when it governs every action.
fn governs_list(c: &Record) -> Option<Vec<String>> {
    c.meta.get("governs").map(|v| v.items().iter().filter_map(Node::scalar).collect())
}

fn governs_json(c: &Record) -> Value {
    json!(governs_list(c))
}

/// " (governs: run, freeze)", or nothing when the commitment governs every action.
fn governs_text(c: &Record) -> String {
    governs_list(c).map(|k| format!(" (governs: {})", k.join(", "))).unwrap_or_default()
}

fn commitment_lines(apps: &[review::Applicable]) -> Vec<String> {
    apps.iter()
        .map(|a| {
            let state = a.commitment.state_str().unwrap_or_default();
            let governs = governs_text(a.commitment);
            format!("  {} [{state}] {}{governs} — via {}", a.commitment.id, doc::take(&a.commitment.title(), 70), a.chains.first().cloned().unwrap_or_default())
        })
        .collect()
}

fn cmd_list(ctx: &Ctx, kind: Option<&str>, state: Option<&str>, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let kind = kind.and_then(OwnKind::parse);
    let mut rows = st.own(kind);
    if let Some(s) = state {
        let want: Vec<&str> = s.split(',').collect();
        rows.retain(|r| r.state_str().is_some_and(|x| want.contains(&x)));
    }
    if json {
        out_json(&Value::Array(
            rows.iter()
                .map(|r| json!({"id": r.id, "kind": r.kind.name(), "title": r.title(), "state": r.state_str(), "rev": r.rev, "path": fsx::rel(&ctx.repo.cfg.base, &r.path)}))
                .collect(),
        ));
    } else {
        for r in rows {
            outln!("{:6} {:13} {}", r.id, r.state_str().unwrap_or_default(), r.title());
        }
    }
    Ok(0)
}

fn cmd_find(ctx: &Ctx, q_text: &str, json: bool) -> Res<i32> {
    let q: Value = serde_json::from_str(q_text).map_err(|e| Fail::code(EXIT_USAGE, format!("query is not JSON: {e}")))?;
    let st = ctx.store()?;
    let mut sorted: Vec<&Record> = st.records.iter().collect();
    sorted.sort_by(|a, b| (!a.is_own()).cmp(&!b.is_own()).then(a.id.cmp(&b.id)));
    let mut hits = vec![];
    for r in sorted {
        let docv = Value::Object(r.to_json(&ctx.repo.cfg.base));
        if query::matches(&docv, &q)? {
            hits.push(r);
        }
    }
    if json {
        out_json(&Value::Array(
            hits.iter()
                .map(|r| {
                    let mut o = r.to_json(&ctx.repo.cfg.base);
                    o.remove("body");
                    o.remove("text");
                    Value::Object(o)
                })
                .collect(),
        ));
    } else {
        for r in hits {
            outln!("{:6} {:11} {:12} {}", r.id, r.kind.name(), r.state_str().unwrap_or_default(), doc::take(&r.title(), 110));
        }
    }
    Ok(0)
}

fn cmd_frontier(ctx: &Ctx, actor: Option<&str>, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let mut fr = views::frontier(&st);
    if let Some(a) = actor {
        fr.retain(|k, _| k == a);
    }
    // with --actor, only that actor's unscheduled threads: the ones it owns but has not queued
    let unscheduled: Vec<_> = views::unscheduled(&st).into_iter().filter(|r| actor.is_none_or(|a| doc::text(&r.meta, "actor").as_deref() == Some(a))).collect();
    let qs: Vec<_> = st.own(Some(OwnKind::Question)).into_iter().filter(|q| q.state_str() == Some("open")).collect();
    let dates = views::dates(&st, chrono::Utc::now().date_naive());
    if json {
        let mut obj = serde_json::Map::new();
        for (k, v) in &fr {
            obj.insert(
                k.clone(),
                Value::Array(v.iter().map(|r| json!({"id": r.id, "next": r.meta.get("next").map(Node::to_json), "title": r.title(), "rev": r.rev})).collect()),
            );
        }
        let unsched: Vec<Value> = unscheduled.iter().map(|r| json!({"id": r.id, "actor": doc::text(&r.meta, "actor"), "title": r.title(), "rev": r.rev})).collect();
        let open: Vec<Value> = qs.iter().map(|q| json!({"id": q.id, "title": q.title(), "rev": q.rev})).collect();
        let dates: Vec<Value> = dates.iter().map(|(r, f, d, passed)| json!({"id": r.id, "field": f, "date": d.to_string(), "passed": passed})).collect();
        out_json(&json!({"queues": obj, "unscheduled": unsched, "open_questions": open, "dates": dates}));
        return Ok(0);
    }
    for (actor_name, rs) in &fr {
        outln!("{actor_name}:");
        for r in rs {
            let n = r.meta.get("next").map(Node::to_string).unwrap_or_default();
            outln!("  {n}. {} {}", r.id, r.title());
        }
    }
    if !unscheduled.is_empty() {
        outln!("open, not scheduled:");
        for r in &unscheduled {
            let who = doc::text(&r.meta, "actor").map(|a| format!(" ({a})")).unwrap_or_default();
            outln!("  {} {}{who}", r.id, r.title());
        }
    }
    if !qs.is_empty() && actor.is_none() {
        outln!("open questions:");
        for q in qs {
            outln!("  {} {}", q.id, q.title());
        }
    }
    if !dates.is_empty() && actor.is_none() {
        outln!("dates:");
        for (r, f, d, passed) in &dates {
            outln!("  {d} {} {f}{}  {}", r.id, if *passed { " (now)" } else { "" }, r.title());
        }
    }
    Ok(0)
}

fn cmd_view(ctx: &Ctx, check_flag: bool, print_flag: bool, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let text = views::render_view(&st);
    let path = ctx.repo.rel(&ctx.repo.cfg.view_path());
    if print_flag {
        if json {
            out_json(&json!({"path": path, "text": text}));
        } else {
            outln!("{text}");
        }
        return Ok(0);
    }
    if check_flag {
        let vp = ctx.repo.cfg.view_path();
        let cur = if vp.exists() { fsx::read_text(&vp).ok() } else { None };
        let current = cur.as_deref() == Some(text.as_str());
        if json {
            out_json(&json!({"path": path, "current": current}));
            return Ok(if current { 0 } else { EXIT_ERROR });
        }
        if !current {
            return Err(Fail::new("the generated view is out of date (run `plied view`)"));
        }
        return Ok(0);
    }
    ctx.writer().regenerate_view(&ctx.policy)?;
    if json {
        out_json(&json!({"path": path}));
    } else {
        outln!("wrote {path}");
    }
    Ok(0)
}

fn cmd_recover(ctx: &Ctx, json: bool) -> Res<i32> {
    let r = ctx.writer().recover()?;
    if json {
        out_json(&match r {
            Some(r) => json!({"recovered": if r.finished { "finished" } else { "undone" }, "files": r.files}),
            None => json!({"recovered": null, "files": 0}),
        });
        return Ok(0);
    }
    match r {
        Some(r) => outln!("{r}"),
        None => outln!("nothing to recover"),
    }
    Ok(0)
}

fn cmd_check(ctx: &Ctx, errors_only: bool, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let msgs = ctx.messages();
    let mut extra: Vec<Problem> = vec![];
    let intent = ctx.repo.runtime.join("intent.json");
    match fsx::pending_intent(&ctx.repo.runtime) {
        Ok(false) => {}
        Ok(true) => extra.push(Problem::error("intent", &intent.display().to_string(), "a write is in progress or was interrupted; run `plied recover`")),
        Err(e) => extra.push(Problem::error("intent", &intent.display().to_string(), e.msg)),
    }

    let lib = match ctx.library_opt() {
        Ok(l) => {
            if let Some(ref l) = l {
                for p in &l.problems {
                    extra.push(Problem::error("manifest", &ctx.repo.cfg.literature.as_ref().unwrap().manifest, p.clone()));
                }
            }
            l
        }
        Err(e) => {
            extra.push(Problem::error("literature", "plied.toml", e.msg.clone()));
            None
        }
    };
    let view = views::render_view(&st);
    let inputs = check::Inputs { library: lib.as_ref(), messages: Some(&msgs), view: Some(&view), warnings: !errors_only };
    let mut probs = extra;
    probs.extend(check::run(&st, &inputs));
    let errs = probs.iter().filter(|p| p.level == Level::Error).count();
    if json {
        out_json(&json!({
            "diagnostics": probs,
            "errors": errs,
            "warnings": probs.len() - errs,
            "records": st.own(None).len(),
            "linked": st.linked().count(),
            "config": ctx.repo.cfg.digest(),
        }));
    } else {
        for p in &probs {
            outln!("{p}");
        }
        outln!(
            "plied {}, config {}: {} records, {} linked; {errs} error(s), {} warning(s)",
            plied::VERSION,
            doc::take(&ctx.repo.cfg.digest(), 12),
            st.own(None).len(),
            st.linked().count(),
            probs.len() - errs
        );
    }
    Ok(if errs > 0 { EXIT_ERROR } else { 0 })
}

fn cmd_inbox(ctx: &Ctx, all: bool, peek: bool, json: bool, who_args: &WhoArgs) -> Res<i32> {
    let who = who_of(who_args, true)?;
    let m = ctx.messages();
    let items = m.inbox(&who.agent, all);
    let mut marked = 0;
    if !peek {
        let unread: Vec<String> = items
            .iter()
            .filter(|e| e.get("read").is_none_or(|v| v.is_null()))
            .filter_map(|e| e.get("id").and_then(Value::as_str).map(String::from))
            .collect();
        if !unread.is_empty() {
            marked = m.mark("read", &unread, &who.agent, who.session.as_deref(), None)?;
        }
    }
    if json {
        out_json(&json!({"messages": items, "marked_read": marked}));
        return Ok(0);
    }
    if items.is_empty() {
        outln!("inbox empty");
    }
    for e in &items {
        let g = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let flags = if !e.get("acked").is_none_or(|v| v.is_null()) {
            "acked"
        } else if !e.get("read").is_none_or(|v| v.is_null()) {
            "read"
        } else {
            "new"
        };
        let reply = e.get("reply_to").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|r| format!(", reply to {r}")).unwrap_or_default();
        outln!("[{}] {} from {} at {} ({flags}{reply})\n  {}", g("id"), g("node"), g("from"), g("at"), g("text").replace('\n', "\n  "));
    }
    Ok(0)
}

fn cmd_messages(ctx: &Ctx, node: &str, json: bool) -> Res<i32> {
    let evs = ctx.messages().events(node);
    if json {
        out_json(&Value::Array(evs.into_iter().map(Value::Object).collect()));
        return Ok(0);
    }
    let mut marks: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for e in &evs {
        let t = e.get("type").and_then(Value::as_str).unwrap_or_default();
        if ["delivered", "read", "ack"].contains(&t) {
            let msg = e.get("msg").and_then(Value::as_str).unwrap_or_default().to_string();
            let agent = e.get("agent").and_then(Value::as_str).unwrap_or_default().to_string();
            marks.entry(msg).or_default().push(format!("{t} {agent}"));
        }
    }
    for e in &evs {
        let t = e.get("type").and_then(Value::as_str).unwrap_or_default();
        let g = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        if t == "message" {
            let reply = e.get("reply_to").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|r| format!(" ↳ {r}")).unwrap_or_default();
            let to = e.get("to").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            outln!("[{}]{reply} {} → {} at {}\n  {}", g("id"), g("from"), if to.is_empty() { "—".into() } else { to }, g("at"), g("text").replace('\n', "\n  "));
            if let Some(mk) = marks.get(&g("id")) {
                let mut u = mk.clone();
                u.sort();
                u.dedup();
                outln!("  ({})", u.join("; "));
            }
        } else if t == "subscribe" || t == "unsubscribe" {
            outln!("  {} {}d at {}", g("agent"), t, g("at"));
        }
    }
    Ok(0)
}

fn cmd_lit(ctx: &Ctx, lit_cmd: &LitCmd, json: bool) -> Res<i32> {
    let lib = ctx.library()?;
    match lit_cmd {
        LitCmd::List => {
            if json {
                out_json(&Value::Array(lib.papers.values().map(|p| json!({"id": p.id, "file": p.file, "pages": lib.pages(p).len()})).collect()));
                return Ok(0);
            }
            for p in lib.papers.values() {
                let pages = lib.pages(p);
                outln!("{:28} {:3} pages  {}", p.id, pages.len(), p.file);
            }
            Ok(0)
        }
        LitCmd::Page { paper, page, image } => {
            let p = lib.paper(paper)?;
            if *image {
                let img = lib.image(p, *page)?;
                if json {
                    out_json(&json!({"paper": paper, "page": page, "image": img.display().to_string()}));
                } else {
                    outln!("{}", img.display());
                }
                return Ok(0);
            }
            match lib.page_text(p, *page)? {
                None => Err(Fail::new(format!("no text for {paper} page {page}"))),
                Some(text) => {
                    let ocr = lib.is_ocr(&text);
                    if json {
                        out_json(&json!({"paper": paper, "page": page, "text": text, "ocr": ocr}));
                        return Ok(0);
                    }
                    if ocr {
                        errln!("[this page's text is OCR: read numbers, tables and figures from the page image]\n");
                    }
                    outln!("{text}");
                    Ok(0)
                }
            }
        }
        LitCmd::Extract { paper } => {
            let n = lib.extract(lib.paper(paper)?)?;
            if json {
                out_json(&json!({"paper": paper, "pages": n}));
            } else {
                outln!("extracted {n} pages");
            }
            Ok(0)
        }
        LitCmd::Cites { paper } => lit_cites(ctx, &lib, paper, json),
        LitCmd::Check => lit_check(ctx, &lib, json),
        LitCmd::Search { terms, paper, n } => cmd_search(ctx, &terms.join(" "), *n, Some("page"), paper.as_deref(), json),
    }
}

fn mention_re(pid: &str) -> Option<regex::Regex> {
    let mut alts = vec![regex::escape(pid)];
    let re = regex::Regex::new(r"^([A-Z][A-Za-z'-]+?)(\d{4})[a-z]*$").ok()?;
    if let Some(m) = re.captures(pid) {
        let (name, year) = (regex::escape(&m[1]), &m[2]);
        alts.push(format!(r"{name}(?:\s+et\s+al\.?|\s+(?:and|&)\s+[A-Z][\w'-]+)?,?\s+\(?{year}"));
    }
    regex::Regex::new(&format!(r"(?<!\w)(?:{})(?!\w)", alts.join("|"))).ok()
}

fn lit_cites(ctx: &Ctx, lib: &Library, pid: &str, json: bool) -> Res<i32> {
    lib.paper(pid)?;
    let st = ctx.store()?;
    let tok = mention_re(pid);
    let mut rows: Vec<(String, String, String)> = vec![];
    let mut cited: Vec<Value> = vec![];
    let mut sorted: Vec<&Record> = st.records.iter().collect();
    sorted.sort_by(|a, b| (!a.is_own()).cmp(&!b.is_own()).then(a.id.cmp(&b.id)));
    for r in sorted {
        let mut pages: Vec<String> = vec![];
        if r.is_own()
            && let Some(Node::List(cites)) = r.meta.get("cites") {
                for c in cites.iter().filter_map(Node::map) {
                    if doc::text(c, "paper").as_deref() == Some(pid) {
                        pages.push(c.get("page").map(Node::to_string).unwrap_or_default());
                    }
                }
            }
        if !pages.is_empty() {
            rows.push((r.id.clone(), format!("cites p{}", pages.join(", p")), r.title()));
            cited.push(json!({"id": r.id, "how": "cites", "pages": pages, "title": r.title()}));
        } else if tok.as_ref().is_some_and(|re| re.is_match(&r.text)) {
            rows.push((r.id.clone(), "mentions".into(), r.title()));
            cited.push(json!({"id": r.id, "how": "mentions", "pages": [], "title": r.title()}));
        }
    }
    if json {
        out_json(&Value::Array(cited));
        return Ok(0);
    }
    for (i, how, t) in &rows {
        outln!("{i:6} {how:18} {}", doc::take(t, 90));
    }
    if rows.is_empty() {
        outln!("nothing cites {pid}");
    }
    Ok(0)
}

fn lit_check(ctx: &Ctx, lib: &Library, json: bool) -> Res<i32> {
    let st = ctx.store()?;
    let mut bad = 0usize;
    let mut all: Vec<Value> = vec![];
    for r in st.own(None) {
        let Some(Node::List(cites)) = r.meta.get("cites") else { continue };
        for c in cites.iter().filter_map(Node::map) {
            let checks = lib.check_citation(c);
            if checks.iter().any(|ch| ch.status == Status::Fail) {
                bad += 1;
            }
            if json {
                let results: Vec<Value> =
                    checks.iter().map(|ch| json!({"aspect": ch.aspect.to_string(), "status": ch.status.to_string(), "message": ch.message})).collect();
                all.push(json!({"record": r.id, "paper": doc::text(c, "paper"), "page": c.get("page").map(Node::to_json), "results": results}));
                continue;
            }
            let line = checks.iter().map(|ch| format!("{}:{}", ch.aspect, ch.status)).collect::<Vec<_>>().join("  ");
            outln!("{:6} {} p{}: {line}", r.id, doc::text(c, "paper").unwrap_or_default(), c.get("page").map(Node::to_string).unwrap_or_default());
            for ch in &checks {
                if matches!(ch.status, Status::Fail | Status::Warn) {
                    outln!("         {}: {}", ch.aspect, ch.message);
                }
            }
        }
    }
    bad += lib.problems.len();
    if json {
        out_json(&json!({"citations": all, "manifest": lib.problems, "failed": bad}));
    } else {
        for msg in &lib.problems {
            outln!("manifest: {msg}");
        }
    }
    Ok(if bad > 0 { EXIT_ERROR } else { 0 })
}

fn cmd_search(ctx: &Ctx, query_text: &str, n: usize, kind: Option<&str>, paper: Option<&str>, json: bool) -> Res<i32> {
    let (hits, notes) = index::search(&ctx.repo.cfg, &ctx.policy, &ctx.repo.runtime, query_text, n, kind, paper)?;
    if json {
        out_json(&Value::Array(
            hits.iter()
                .map(|h| json!({"key": h.key, "kind": h.kind, "path": ctx.repo.rel(&h.path), "score": h.score, "snippet": h.snippet, "stale": h.stale, "ocr": h.ocr}))
                .collect(),
        ));
    } else {
        for h in &hits {
            let flags = format!("{}{}", if h.stale { " [changed since indexed]" } else { "" }, if h.ocr { " [OCR: check the page image]" } else { "" });
            outln!("{}  ({:.1}){flags}\n  {}\n", h.key, h.score, h.snippet);
        }
        if hits.is_empty() {
            outln!("no matches");
        }
    }
    for n in notes {
        plied::notes::note(n);
    }
    Ok(0)
}

fn cmd_review(ctx: &Ctx, nodes: &[String], action: Option<&str>, gate: bool, json: bool) -> Res<i32> {
    if gate && action.is_none() {
        return Err(Fail::code(EXIT_USAGE, "--gate needs --action: a disposition counts only for the action it was recorded for"));
    }
    if let Some(msg) = action.and_then(|a| ctx.repo.cfg.action_problem(a)) {
        return Err(Fail::code(EXIT_USAGE, msg));
    }
    let st = ctx.store()?;
    for n in nodes {
        if st.get(n).is_none() {
            return Err(Fail::new(format!("no record {n}")));
        }
    }
    let items = review::applicable(&st, nodes, action);
    let reasons = match (gate, action) {
        (true, Some(a)) => Some(review::gate(&items, a)),
        _ => None,
    };
    if json {
        let commitments: Vec<Value> = items
            .iter()
            .map(|x| {
                json!({
                    "id": x.commitment.id, "rev": x.commitment.rev,
                    "state": x.commitment.state_str(),
                    "wording": doc::text(&x.commitment.meta, "wording"),
                    "chains": x.chains,
                    "reviews": x.reviews.iter().map(|rv| Value::Object(doc::map_to_json(rv))).collect::<Vec<_>>(),
                    "governs": governs_json(x.commitment),
                    "governs_action": x.governs_action,
                })
            })
            .collect();
        let gate_v = reasons.as_ref().map(|r| json!({"action": action, "passed": r.is_empty(), "reasons": r}));
        out_json(&json!({"commitments": commitments, "gate": gate_v}));
    } else {
        let (governed, others): (Vec<&review::Applicable>, Vec<&review::Applicable>) = items.iter().partition(|x| x.governs_action);
        if governed.is_empty() {
            let gov = if others.is_empty() { "" } else { " and governs this action" };
            outln!("no open commitment applies to {}{gov}", nodes.join(", "));
        }
        for x in &governed {
            let c = x.commitment;
            outln!("{} [{}] rev {}  {}", c.id, c.state_str().unwrap_or_default(), &c.rev[..12.min(c.rev.len())], c.title());
            outln!("  applies via: {}", x.chains.join("; "));
            outln!("  wording ({}): “{}”", doc::text(&c.meta, "made").unwrap_or_default(), doc::text(&c.meta, "wording").unwrap_or_default());
            if let Some(t) = doc::text(&c.meta, "trigger") {
                outln!("  trigger: {t}");
            }
            for rv in &x.reviews {
                outln!(
                    "  disposition for {}: {} — {} ({}, {})",
                    doc::text(rv, "action").unwrap_or_default(),
                    doc::text(rv, "disposition").unwrap_or_default(),
                    doc::text(rv, "because").unwrap_or_default(),
                    doc::text(rv, "by").unwrap_or_default(),
                    doc::text(rv, "at").unwrap_or_default()
                );
            }
            if x.reviews.is_empty()
                && let Some(a) = action
            {
                outln!("  → record: plied dispose {} --action {} --disposition … --because \"…\" --expect {}", c.id, serde_json::to_string(a).unwrap_or_default(), &c.rev[..12.min(c.rev.len())]);
            }
            outln!();
        }
        if let (Some(a), false) = (action, others.is_empty()) {
            outln!("Applying, but not governing {} actions, so the gate leaves them out:", config::Config::action_kind(a));
            for x in &others {
                let c = x.commitment;
                outln!("  {} [{}] {}{}", c.id, c.state_str().unwrap_or_default(), c.title(), governs_text(c));
            }
        }
    }
    if let Some(reasons) = reasons {
        if !json {
            for r in &reasons {
                errln!("gate: {r}");
            }
        }
        return Ok(if reasons.is_empty() { 0 } else { EXIT_GATE });
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn cmd_dispose(ctx: &Ctx, id: &str, action: &str, disposition: &str, because: &str, ref_: Option<&str>, expect: &str, who_args: &WhoArgs, json: bool) -> Res<i32> {
    use plied::records::Disposition;
    if Disposition::parse(disposition).is_none() {
        return Err(Fail::code(EXIT_USAGE, format!("--disposition must be one of {}", Disposition::ALL.join(", "))));
    }
    if let Some(msg) = ctx.repo.cfg.action_problem(action) {
        return Err(Fail::code(EXIT_USAGE, msg));
    }
    let who = who_of(who_args, true)?;
    let who_s = who.to_string();
    let (action_s, disp_s, because_s, ref_s) = (action.to_string(), disposition.to_string(), because.to_string(), ref_.map(String::from));
    let mut mutate = |meta: &mut Map| -> Res<String> {
        let mut entry = Map::new();
        entry.insert("action".into(), Node::Str(action_s.clone()));
        entry.insert("disposition".into(), Node::Str(disp_s.clone()));
        entry.insert("because".into(), Node::Str(because_s.clone()));
        entry.insert("by".into(), Node::Str(who_s.clone()));
        entry.insert("at".into(), Node::Str(plied::records::stamp()));
        if let Some(r) = &ref_s {
            entry.insert("ref".into(), Node::Str(r.clone()));
        }
        let mut reviews: Vec<Node> = meta.get("reviews").map(|v| v.items().to_vec()).unwrap_or_default();
        reviews.push(Node::Map(entry));
        meta.insert("reviews".into(), Node::List(reviews));
        Ok(format!("review for {action_s}: {disp_s}"))
    };
    let lib = ctx.library_opt()?;
    let rev = ctx.writer().update(&ctx.policy, lib.as_ref(), id, expect, &who, Some(because), &[], None, None, Some(&mut mutate))?;
    if json {
        out_json(&json!({"id": id, "rev": rev}));
    } else {
        outln!("{id}: disposition recorded (rev {})", &rev[..12.min(rev.len())]);
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn cmd_scanned(ctx: &Ctx, id: &str, rev: &str, on: &str, because: &str, expect: &str, who_args: &WhoArgs, json: bool) -> Res<i32> {
    let rev = rev.to_ascii_lowercase();
    if !plied::records::is_rev(&rev) {
        return Err(Fail::code(EXIT_USAGE, "--rev needs the revision you read (at least 8 hex characters; `plied show` prints it)"));
    }
    let st = ctx.store()?;
    let r = st.get(id).ok_or_else(|| Fail::new(format!("no record {id}")))?;
    let Some(src) = r.source() else {
        return Err(Fail::new(format!("{} is a plied record; only linked records are scanned for commitments", r.id)));
    };
    if !src.scan_commitments {
        return Err(Fail::new(format!("{} is a {} record, and that source does not set scan_commitments", r.id, src.name)));
    }
    // a review covers what was read: a record that moved on since must be read again
    if !r.rev.starts_with(&rev) {
        let msg = format!("{} changed since you read it (expected {}, now {}); read it again", r.id, doc::take(&rev, 12), &r.rev[..12]);
        return Err(Fail::code(plied::error::EXIT_CONFLICT, msg).with_detail(json!({"id": r.id, "expected": rev, "current": r.rev})));
    }
    if !st.get(on).is_some_and(|t| t.is(OwnKind::Thread)) {
        return Err(Fail::new(format!("{on} is not a thread")));
    }
    let who = who_of(who_args, true)?;
    let who_s = who.to_string();
    let (rid, full_rev, because_s) = (r.id.clone(), r.rev.clone(), because.to_string());
    let mut mutate = |meta: &mut Map| -> Res<String> {
        let mut entry = Map::new();
        entry.insert("record".into(), Node::Str(rid.clone()));
        entry.insert("rev".into(), Node::Str(full_rev.clone()));
        entry.insert("at".into(), Node::Str(plied::records::stamp()));
        entry.insert("by".into(), Node::Str(who_s.clone()));
        entry.insert("because".into(), Node::Str(because_s.clone()));
        let mut scanned: Vec<Node> = meta.get("scanned").map(|v| v.items().to_vec()).unwrap_or_default();
        scanned.push(Node::Map(entry));
        meta.insert("scanned".into(), Node::List(scanned));
        Ok(format!("scanned {rid} (rev {}): no commitment to record", &full_rev[..12]))
    };
    let lib = ctx.library_opt()?;
    let new_rev = ctx.writer().update(&ctx.policy, lib.as_ref(), on, expect, &who, Some(because), &[], None, None, Some(&mut mutate))?;
    if json {
        out_json(&json!({"id": on, "rev": new_rev}));
    } else {
        outln!("{on}: scan of {} recorded (rev {})", r.id, &new_rev[..12.min(new_rev.len())]);
    }
    Ok(0)
}

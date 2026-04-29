//! `rongnote` CLI — talks to a rongnote-server with full client-side
//! crypto. Drop-in equivalent of the browser app for headless workflows.

mod api;
mod config;
mod crypto;

use anyhow::{anyhow, bail, Context, Result};
use api::{
    validate_item_type, ApiClient, CreateItemInput, Item, ItemSummary, ListItemsOptions,
    MemberKeyInput,
};
use clap::{Parser, Subcommand};
use config::Session;
use std::io::{self, Read, Write};

const DEFAULT_SERVER: &str = "https://notes.ronglab.de";

#[derive(Parser, Debug)]
#[command(
    name = "rongnote",
    about = "End-to-end encrypted info hub — CLI client",
    version
)]
struct Cli {
    /// Override the server URL (default: the one used at last login, else
    /// $RONGNOTE_SERVER, else https://notes.ronglab.de).
    #[arg(long, global = true)]
    server: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Log in with email + passphrase. Stores the unwrapped master_key
    /// locally so subsequent commands don't re-prompt.
    Login {
        #[arg(short, long)]
        email: Option<String>,
    },
    /// Drop the local session + clear server-side cookie.
    Logout,
    /// Show whether you're logged in and which space is active.
    Status,
    /// List items in the active space (optionally filtered).
    Ls {
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        path: Option<String>,
        /// Show items in the trash instead.
        #[arg(long)]
        trash: bool,
    },
    /// Decrypt an item's body and print to stdout.
    Cat {
        id: String,
    },
    /// Create a new item. Body is read from stdin (or --body).
    New {
        /// note | task | snippet | bookmark | secret
        kind: String,
        /// Title.
        title: String,
        /// Inline body. Mutually exclusive with stdin.
        #[arg(long)]
        body: Option<String>,
        /// Comma-separated tags.
        #[arg(long)]
        tags: Option<String>,
        /// Path (defaults to /).
        #[arg(long)]
        path: Option<String>,
        /// 'YYYY-MM-DD' for tasks.
        #[arg(long)]
        due: Option<String>,
    },
    /// Move an item to trash (or pass --hard to delete permanently).
    Rm {
        id: String,
        #[arg(long)]
        hard: bool,
    },
    /// List all spaces you're a member of.
    Spaces,
    /// Switch the active space for subsequent commands.
    Use {
        /// Space id or exact name match (case-sensitive).
        space: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let server_override = cli
        .server
        .or_else(|| std::env::var("RONGNOTE_SERVER").ok());
    match cli.command {
        Command::Login { email } => cmd_login(server_override, email),
        Command::Logout => cmd_logout(server_override),
        Command::Status => cmd_status(server_override),
        Command::Ls {
            r#type,
            tag,
            path,
            trash,
        } => cmd_ls(server_override, r#type, tag, path, trash),
        Command::Cat { id } => cmd_cat(server_override, id),
        Command::New {
            kind,
            title,
            body,
            tags,
            path,
            due,
        } => cmd_new(server_override, kind, title, body, tags, path, due),
        Command::Rm { id, hard } => cmd_rm(server_override, id, hard),
        Command::Spaces => cmd_spaces(server_override),
        Command::Use { space } => cmd_use(server_override, space),
    }
}

// --- Session helpers ---

fn pick_server(cli_or_env: Option<String>, persisted: Option<&str>) -> String {
    cli_or_env
        .or_else(|| persisted.map(str::to_owned))
        .unwrap_or_else(|| DEFAULT_SERVER.to_owned())
}

fn require_session(server_override: Option<String>) -> Result<(ApiClient, Session)> {
    let saved = Session::load()?
        .ok_or_else(|| anyhow!("not logged in — run `rongnote login` first"))?;
    let server = pick_server(server_override, Some(&saved.server));
    let client = ApiClient::new(&server)?;
    client.restore_cookie(&saved.cookie)?;
    Ok((client, saved))
}

// --- Commands ---

fn cmd_login(server_override: Option<String>, email: Option<String>) -> Result<()> {
    let server = pick_server(server_override, None);
    let client = ApiClient::new(&server)?;

    let email = match email {
        Some(e) => e,
        None => prompt_line("email: ")?,
    };
    let passphrase = rpassword::prompt_password("passphrase: ")
        .context("reading passphrase")?;

    eprintln!("deriving keys (Argon2id, ~1s)…");
    let pre = client.precheck(&email)?;
    let salt = crypto::b64_decode(&pre.passphrase_salt)?;
    let kek = crypto::derive_kek_from_passphrase(&passphrase, &salt)?;
    let wrap = crypto::b64_decode(&pre.master_wrap_passphrase)?;
    let master_key_vec = crypto::open(&wrap, &kek)
        .map_err(|_| anyhow!("wrong passphrase"))?;
    let master_key: [u8; crypto::KEY_LEN] = master_key_vec
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("master_key size mismatch"))?;

    let auth_hash = crypto::derive_auth_hash(&master_key)?;
    let user = client.login(&email, &crypto::b64_encode(&auth_hash))?;

    // Unwrap the X25519 private key with the master_key.
    let enc_priv = crypto::b64_decode(&user.encrypted_private_key)?;
    let priv_vec = crypto::open(&enc_priv, &master_key)
        .map_err(|_| anyhow!("private key unwrap failed"))?;

    let cookie = client
        .current_session_cookie()
        .ok_or_else(|| anyhow!("server did not set a session cookie"))?;

    let mut session = Session {
        server,
        email: user.email.clone(),
        user_id: user.id.clone(),
        cookie,
        master_key_b64: crypto::b64_encode(&master_key),
        public_key_b64: user.public_key.clone(),
        private_key_b64: crypto::b64_encode(&priv_vec),
        active_space_id: None,
    };

    // Pick personal space as the default active.
    if let Ok(spaces) = client.list_spaces() {
        if let Some(personal) = spaces.iter().find(|s| s.kind == "personal") {
            session.active_space_id = Some(personal.id.clone());
        } else if let Some(first) = spaces.first() {
            session.active_space_id = Some(first.id.clone());
        }
    }

    session.save()?;
    println!("logged in as {} ({})", user.email, server_short(&session.server));
    Ok(())
}

fn cmd_logout(server_override: Option<String>) -> Result<()> {
    if let Some(saved) = Session::load()? {
        let server = pick_server(server_override, Some(&saved.server));
        let client = ApiClient::new(&server)?;
        client.restore_cookie(&saved.cookie)?;
        let _ = client.logout(); // best-effort
    }
    Session::clear()?;
    println!("logged out");
    Ok(())
}

fn cmd_status(server_override: Option<String>) -> Result<()> {
    let saved = match Session::load()? {
        Some(s) => s,
        None => {
            println!("not logged in");
            return Ok(());
        }
    };
    let server = pick_server(server_override, Some(&saved.server));
    let client = ApiClient::new(&server)?;
    client.restore_cookie(&saved.cookie)?;
    let me = client.me().context("server says you're not logged in")?;
    println!("logged in as {} ({})", me.email, server_short(&server));
    if let Some(active) = &saved.active_space_id {
        let spaces = client.list_spaces().unwrap_or_default();
        let name = spaces
            .iter()
            .find(|s| &s.id == active)
            .map(|s| format!("{} ({})", s.name, s.kind))
            .unwrap_or_else(|| active.clone());
        println!("active space: {name}");
    }
    Ok(())
}

fn cmd_ls(
    server_override: Option<String>,
    type_: Option<String>,
    tag: Option<String>,
    path: Option<String>,
    trash: bool,
) -> Result<()> {
    let (client, session) = require_session(server_override)?;
    if let Some(t) = &type_ {
        validate_item_type(t)?;
    }
    let opts = ListItemsOptions {
        type_,
        trash,
        space_id: session.active_space_id.clone(),
    };
    let items = client.list_items(&opts)?;
    let filtered: Vec<&ItemSummary> = items
        .iter()
        .filter(|i| match &tag {
            Some(t) => i.tags.iter().any(|x| x == t),
            None => true,
        })
        .filter(|i| match &path {
            Some(p) => i.path == *p || i.path.starts_with(&format!("{p}/")),
            None => true,
        })
        .collect();

    let id_w = filtered.iter().map(|i| i.id.len().min(8)).max().unwrap_or(8);
    let type_w = filtered.iter().map(|i| i.type_.len()).max().unwrap_or(4).max(4);
    for i in &filtered {
        let mark = if i.pinned {
            "★"
        } else if i.done {
            "✓"
        } else {
            " "
        };
        println!(
            "{:<idw$}  {mark} {:<typew$}  {}  {}",
            &i.id[..id_w],
            i.type_,
            shorten(&i.path, 16),
            i.title,
            idw = id_w,
            typew = type_w,
        );
    }
    Ok(())
}

fn cmd_cat(server_override: Option<String>, id: String) -> Result<()> {
    let (client, session) = require_session(server_override)?;
    let item = client.get_item(&id)?;
    let body = decrypt_body(&item, &session)?;
    io::stdout().write_all(body.as_bytes())?;
    Ok(())
}

fn cmd_new(
    server_override: Option<String>,
    kind: String,
    title: String,
    body_arg: Option<String>,
    tags_arg: Option<String>,
    path_arg: Option<String>,
    due: Option<String>,
) -> Result<()> {
    validate_item_type(&kind)?;
    let (client, session) = require_session(server_override)?;
    let space_id = session
        .active_space_id
        .clone()
        .ok_or_else(|| anyhow!("no active space — run `rongnote spaces` then `rongnote use <id>`"))?;

    // Body comes from --body, else stdin if it isn't a TTY.
    let body_text = match body_arg {
        Some(b) => b,
        None => read_stdin_or_empty()?,
    };

    // Most types want a JSON-shaped body; for `note` we ship raw markdown.
    // The browser editors serialize per-type payloads, but for the CLI we
    // only support note creation with arbitrary text. Other types still
    // get a basic body (caller can also leave empty).
    let body_for_api: String = match kind.as_str() {
        "note" => body_text.clone(),
        "task" => serde_json::json!({"description": body_text}).to_string(),
        "snippet" => {
            serde_json::json!({"language": "", "code": body_text, "description": ""}).to_string()
        }
        "bookmark" => serde_json::json!({"url": body_text.trim(), "description": ""}).to_string(),
        "list" => serde_json::json!({"entries": []}).to_string(),
        "secret" => bail!("creating secrets via CLI not supported in v1 (need --user/--pass)"),
        "file" => bail!("file uploads via CLI not supported in v1"),
        "event" => bail!("event creation not supported"),
        _ => body_text.clone(),
    };

    // Wrap with sealed-box-per-member if the active space is a team space,
    // otherwise master-key secretbox.
    let master_key = decode_session_key(&session.master_key_b64)?;
    let item_key = random_key();
    let encrypted_body = crypto::seal(body_for_api.as_bytes(), &item_key)?;
    let encrypted_body_b64 = crypto::b64_encode(&encrypted_body);

    let spaces = client.list_spaces()?;
    let space = spaces
        .iter()
        .find(|s| s.id == space_id)
        .ok_or_else(|| anyhow!("active space {space_id} not found"))?;

    let (wrapped_item_key, member_keys) = if space.kind == "team" {
        let members = client.list_members(&space_id)?;
        let mut wraps = Vec::with_capacity(members.len());
        for m in members {
            let pk: [u8; crypto::KEY_LEN] = crypto::b64_decode_array(&m.public_key)?;
            let sealed = crypto::box_seal(&item_key, &pk)?;
            wraps.push(MemberKeyInput {
                user_id: m.user_id,
                sealed_item_key: crypto::b64_encode(&sealed),
            });
        }
        (None, Some(wraps))
    } else {
        let wrapped = crypto::seal(&item_key, &master_key)?;
        (Some(crypto::b64_encode(&wrapped)), None)
    };

    let tags = tags_arg
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().trim_start_matches('#').to_lowercase())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let input = CreateItemInput {
        type_: Some(kind.clone()),
        title,
        encrypted_body: Some(encrypted_body_b64),
        wrapped_item_key,
        member_keys,
        tags: Some(tags),
        path: path_arg,
        due_at: due,
        space_id: Some(space_id),
    };
    let created = client.create_item(&input)?;
    println!("{}", created.id);
    Ok(())
}

fn cmd_rm(server_override: Option<String>, id: String, hard: bool) -> Result<()> {
    let (client, _session) = require_session(server_override)?;
    client.delete_item(&id, hard)?;
    println!("{} {}", if hard { "permanently deleted" } else { "trashed" }, id);
    Ok(())
}

fn cmd_spaces(server_override: Option<String>) -> Result<()> {
    let (client, session) = require_session(server_override)?;
    let spaces = client.list_spaces()?;
    let id_w = spaces.iter().map(|s| s.id.len().min(8)).max().unwrap_or(8);
    for s in &spaces {
        let active = if Some(&s.id) == session.active_space_id.as_ref() {
            "*"
        } else {
            " "
        };
        println!(
            "{} {:<idw$}  {:<8}  {:>2} member{}  {}",
            active,
            &s.id[..id_w],
            s.kind,
            s.member_count,
            if s.member_count == 1 { "" } else { "s" },
            s.name,
            idw = id_w,
        );
    }
    Ok(())
}

fn cmd_use(server_override: Option<String>, space: String) -> Result<()> {
    let (client, mut session) = require_session(server_override)?;
    let spaces = client.list_spaces()?;
    let target = spaces
        .iter()
        .find(|s| s.id == space || s.name == space || s.id.starts_with(&space))
        .ok_or_else(|| anyhow!("no space matching {space:?}"))?;
    session.active_space_id = Some(target.id.clone());
    session.save()?;
    println!("active space: {} ({})", target.name, target.kind);
    Ok(())
}

// --- Decryption helpers ---

fn decode_session_key(b64: &str) -> Result<[u8; crypto::KEY_LEN]> {
    crypto::b64_decode_array(b64).context("session master_key invalid")
}

fn random_key() -> [u8; crypto::KEY_LEN] {
    use rand_core::{OsRng, RngCore};
    let mut k = [0u8; crypto::KEY_LEN];
    OsRng.fill_bytes(&mut k);
    k
}

fn decrypt_body(item: &Item, session: &Session) -> Result<String> {
    let body_b64 = item
        .encrypted_body
        .as_deref()
        .ok_or_else(|| anyhow!("item has no body"))?;
    let wrap_b64 = item
        .wrapped_item_key
        .as_deref()
        .ok_or_else(|| anyhow!("item has no key wrap"))?;
    let kind = item.key_wrap.as_deref().unwrap_or("master");
    let wrap = crypto::b64_decode(wrap_b64)?;
    let item_key_vec = match kind {
        "sealed" => {
            let pk: [u8; crypto::KEY_LEN] = crypto::b64_decode_array(&session.public_key_b64)?;
            let sk: [u8; crypto::KEY_LEN] = crypto::b64_decode_array(&session.private_key_b64)?;
            crypto::box_open(&wrap, &pk, &sk)?
        }
        _ => {
            let mk: [u8; crypto::KEY_LEN] = decode_session_key(&session.master_key_b64)?;
            crypto::open(&wrap, &mk)?
        }
    };
    let item_key: [u8; crypto::KEY_LEN] = item_key_vec
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("item_key wrong size"))?;
    let body = crypto::open(&crypto::b64_decode(body_b64)?, &item_key)?;
    String::from_utf8(body).map_err(|e| anyhow!("body is not valid utf-8: {e}"))
}

// --- I/O utilities ---

fn prompt_line(prompt: &str) -> Result<String> {
    use std::io::BufRead;
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut s = String::new();
    io::stdin().lock().read_line(&mut s)?;
    Ok(s.trim().to_owned())
}

fn read_stdin_or_empty() -> Result<String> {
    if atty_stdin() {
        return Ok(String::new());
    }
    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf)?;
    Ok(buf)
}

/// Cheap "is stdin a tty?" without an extra dep — peek at the file
/// descriptor / handle via std. Fall back to "not a tty" if we can't tell.
fn atty_stdin() -> bool {
    use std::io::IsTerminal;
    io::stdin().is_terminal()
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

fn server_short(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| url.to_owned())
}

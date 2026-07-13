//! Speaking to the Windows Search index — the OLE DB half of the content provider.
//!
//! Two COM stacks, in sequence, because that is how Windows splits the job:
//!
//! 1. **The Search API** (`ISearchManager` → `ISearchCatalogManager` → `ISearchQueryHelper`)
//!    turns what the user typed into SQL. It is the piece worth having: it parses Advanced
//!    Query Syntax, quotes and escapes the terms, and knows the column names. Hand-writing
//!    that SQL would mean hand-writing the escaping, and a query is user input.
//! 2. **OLE DB** (`Search.CollatorDSO`) runs it. There is no friendlier surface — the
//!    collator is an OLE DB provider and nothing else, so the accessor/binding dance below is
//!    the price of admission. It is confined to this file.
//!
//! **It all lives on one worker thread**, and the thread outlives the query. Three reasons,
//! in order of how much they hurt: COM must be initialized on whatever thread calls it, and
//! the orchestrator hands each query a *fresh* thread that then exits — initializing an
//! apartment per keystroke to abandon it a moment later is not a thing to do. The
//! `ISearchManager` is an out-of-process object, so paying to create it again on every
//! keystroke is real money. And the thread lets stale questions be dropped: when several
//! queries are waiting, only the newest is still being asked.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use windows::core::{w, Interface, GUID, HSTRING, PWSTR};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Search::{
    CSearchManager, IAccessor, ICommand, ICommandText, IDBCreateCommand, IDBCreateSession, IDBInitialize, IRowset,
    ISearchQueryHelper, DBACCESSOR_ROWDATA, DBBINDING, DBMEMOWNER_CLIENTOWNED, DBPARAMIO_NOTPARAM, DBPART_LENGTH,
    DBPART_STATUS, DBPART_VALUE, DBSTATUS_S_OK, DBTYPE_WSTR, HACCESSOR,
};

/// The command dialect `ICommandText` is handed: "whatever the provider's default is", which
/// for the collator is Windows Search SQL. Not in the `windows` crate (`oledb.h` defines it as
/// a plain GUID constant), so it is spelled out here.
const DBGUID_DEFAULT: GUID = GUID::from_u128(0xc8b521fb_5cf3_11ce_ade5_00aa0044773d);

/// The one column we ask for. Everything else about a row — its name, its icon — is derived
/// from the path, so a second binding would buy nothing.
///
/// **Not `System.ItemPathDisplay`**, however much its name suggests otherwise. That column is
/// *display* text, and Windows localizes it: on a German machine it hands back
/// `C:\Benutzer\bened\…` for a file that lives at `C:\Users\bened\…`. Explorer paints that
/// name; the filesystem has never heard of it, so every row would have opened nothing.
/// `System.ItemUrl` is the real location ([`path_from_url`] unwraps it).
const SELECT_COLUMNS: &str = "System.ItemUrl";

/// Relevance order. Without it the index answers in its own order, which is not an order at
/// all as far as the user is concerned.
const SORT: &str = "System.Search.Rank DESC";

/// Search *inside* files, and only inside them.
///
/// The default is to match the query against every indexed property, path and filename
/// included — which would make `ff` a worse, slower `f`: ask for `ff funke` and every file
/// under a folder called `funke` comes back, burying the documents that actually say the
/// word. `f` already owns names. This provider owns what is written in the file, and the
/// section header promises exactly that.
const CONTENT_PROPERTIES: &str = "System.Search.Contents";

/// Room for a path. Longer than `MAX_PATH` because the index is full of things that are;
/// anything past this is reported truncated by the provider and skipped.
const PATH_CHARS: usize = 1024;

/// How long a caller waits for the worker. Generously past anything a healthy index takes —
/// this is a backstop against a wedged indexer, not a deadline. The *deadline* is the
/// orchestrator's, and it is 120 ms: a query that takes longer than that simply arrives late.
const REPLY_TIMEOUT: Duration = Duration::from_secs(4);

/// The Windows Search index did not answer. The service can be stopped, disabled by policy,
/// or rebuilding — all of which are the user's business and none of which are Funke's to fix.
/// The provider returns no rows and says so once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unavailable;

struct Request {
    text: String,
    roots: Vec<PathBuf>,
    max: usize,
    reply: Sender<Result<Vec<PathBuf>, Unavailable>>,
}

/// A handle to the search thread. Cheap to hold, safe to share.
pub struct WinSearch {
    tx: Sender<Request>,
}

impl WinSearch {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || worker(rx));
        Self { tx }
    }

    /// Paths of indexed files whose contents match `text`, best match first, scoped to
    /// `roots`.
    pub fn query(&self, text: &str, roots: &[PathBuf], max: usize) -> Result<Vec<PathBuf>, Unavailable> {
        let (reply, answer) = mpsc::channel();
        let request = Request {
            text: text.to_string(),
            roots: roots.to_vec(),
            max,
            reply,
        };
        if self.tx.send(request).is_err() {
            return Err(Unavailable); // The worker thread is gone — it never comes back.
        }
        match answer.recv_timeout(REPLY_TIMEOUT) {
            Ok(result) => result,
            // Timed out, or the worker dropped this request because a newer one had already
            // arrived. Either way there are no rows for this keystroke, and the next one will
            // have them.
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Ok(Vec::new()),
        }
    }
}

fn worker(rx: Receiver<Request>) {
    // MTA: the calls below are outgoing-only and the collator is in-process, but
    // `ISearchManager` is not — and an STA thread with no message pump has no business
    // holding cross-apartment proxies.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut backend: Option<Backend> = None;
    let mut complained = false;

    while let Ok(mut request) = rx.recv() {
        // Only the newest question is still being asked. The callers behind the older ones
        // have been superseded by a keystroke; their receivers are already being dropped.
        while let Ok(newer) = rx.try_recv() {
            request = newer;
        }

        let result = unsafe { answer(&mut backend, &request) };
        let reply = result.map_err(|error| {
            // Whatever went wrong, the COM objects we cached may be part of it (a restarted
            // indexer leaves stale proxies behind). Drop them; the next query rebuilds.
            backend = None;
            if !complained {
                complained = true;
                eprintln!("content search unavailable — the Windows Search index did not answer: {error}");
            }
            Unavailable
        });
        let _ = request.reply.send(reply);
    }
}

/// The COM objects worth keeping between queries. Both are expensive to create and neither
/// carries any state from one query to the next except the settings we want anyway.
struct Backend {
    helper: ISearchQueryHelper,
    db: IDBInitialize,
}

impl Drop for Backend {
    fn drop(&mut self) {
        unsafe {
            let _ = self.db.Uninitialize();
        }
    }
}

unsafe fn answer(backend: &mut Option<Backend>, request: &Request) -> windows::core::Result<Vec<PathBuf>> {
    let backend = match backend {
        Some(backend) => backend,
        none => none.insert(connect()?),
    };

    backend.helper.SetQueryMaxResults(request.max as i32)?;
    backend
        .helper
        .SetQueryWhereRestrictions(&HSTRING::from(scope_clause(&request.roots)))?;
    let sql = take_string(backend.helper.GenerateSQLFromUserQuery(&HSTRING::from(&request.text))?);
    if sql.is_empty() {
        return Ok(Vec::new());
    }
    rows(&backend.db, &sql, request.max)
}

/// Open both halves: the query helper that writes the SQL, and the data source that runs it.
unsafe fn connect() -> windows::core::Result<Backend> {
    let manager: windows::Win32::System::Search::ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_ALL)?;
    let catalog = manager.GetCatalog(w!("SystemIndex"))?;
    let helper = catalog.GetQueryHelper()?;
    helper.SetQuerySelectColumns(&HSTRING::from(SELECT_COLUMNS))?;
    helper.SetQuerySorting(&HSTRING::from(SORT))?;
    helper.SetQueryContentProperties(&HSTRING::from(CONTENT_PROPERTIES))?;

    // The collator is an in-process OLE DB provider; the ProgID spares us a hard-coded CLSID.
    let db: IDBInitialize = CoCreateInstance(&CLSIDFromProgID(w!("Search.CollatorDSO"))?, None, CLSCTX_INPROC_SERVER)?;
    db.Initialize()?;
    Ok(Backend { helper, db })
}

/// Run one statement and read the path column out of every row.
unsafe fn rows(db: &IDBInitialize, sql: &str, max: usize) -> windows::core::Result<Vec<PathBuf>> {
    let session: IDBCreateSession = db.cast()?;
    let creator: IDBCreateCommand = session.CreateSession(None, &IDBCreateCommand::IID)?.cast()?;
    let command: ICommandText = creator.CreateCommand(None, &ICommandText::IID)?.cast()?;
    command.SetCommandText(&DBGUID_DEFAULT, &HSTRING::from(sql))?;

    let mut rowset = None;
    command
        .cast::<ICommand>()?
        .Execute(None, &IRowset::IID, None, None, Some(&mut rowset))?;
    let Some(rowset) = rowset else {
        return Ok(Vec::new()); // A statement that matched nothing has no rowset at all.
    };
    let rowset: IRowset = rowset.cast()?;

    let accessor: IAccessor = rowset.cast()?;
    let binding = path_binding();
    let mut haccessor = HACCESSOR::default();
    accessor.CreateAccessor(
        DBACCESSOR_ROWDATA.0 as u32,
        1,
        &binding,
        std::mem::size_of::<RowBuf>(),
        &mut haccessor,
        None,
    )?;
    let paths = read_rows(&rowset, haccessor, max);
    let _ = accessor.ReleaseAccessor(haccessor, None);
    Ok(paths)
}

/// One row's worth of the path column, laid out the way the binding below describes it. The
/// provider writes straight into this, so its shape *is* the contract — hence `repr(C)` and
/// `offset_of!` rather than three hand-counted numbers that would rot apart.
#[repr(C)]
struct RowBuf {
    status: u32,
    /// Bytes, not characters — OLE DB counts `DBTYPE_WSTR` in bytes.
    length: usize,
    path: [u16; PATH_CHARS],
}

fn path_binding() -> DBBINDING {
    DBBINDING {
        iOrdinal: 1, // The first (and only) selected column.
        obValue: std::mem::offset_of!(RowBuf, path),
        obLength: std::mem::offset_of!(RowBuf, length),
        obStatus: std::mem::offset_of!(RowBuf, status),
        pTypeInfo: std::mem::ManuallyDrop::new(None),
        pObject: std::ptr::null_mut(),
        pBindExt: std::ptr::null_mut(),
        dwPart: (DBPART_VALUE.0 | DBPART_LENGTH.0 | DBPART_STATUS.0) as u32,
        dwMemOwner: DBMEMOWNER_CLIENTOWNED.0 as u32,
        eParamIO: DBPARAMIO_NOTPARAM.0 as u32,
        cbMaxLen: std::mem::size_of::<[u16; PATH_CHARS]>(),
        dwFlags: 0,
        wType: DBTYPE_WSTR.0 as u16,
        bPrecision: 0,
        bScale: 0,
    }
}

unsafe fn read_rows(rowset: &IRowset, haccessor: HACCESSOR, max: usize) -> Vec<PathBuf> {
    // `windows` folds OLE DB's row count and its out-pointer into one slice: the length is
    // how many rows are asked for, and the provider writes the address of the array it
    // allocated into element zero. The rest of the slice is never touched.
    let mut handles: Vec<*mut usize> = vec![std::ptr::null_mut(); max];
    let mut obtained: usize = 0;
    if rowset.GetNextRows(0, 0, &mut obtained, &mut handles).is_err() || obtained == 0 {
        return Vec::new();
    }
    let hrows = handles[0];

    let mut buffer: RowBuf = std::mem::zeroed();
    let mut paths = Vec::with_capacity(obtained);
    for row in 0..obtained {
        if rowset
            .GetData(*hrows.add(row), haccessor, std::ptr::from_mut(&mut buffer).cast())
            .is_err()
        {
            continue;
        }
        // Anything but a whole value — NULL, or a path longer than the buffer — is skipped:
        // half a path is not a file anyone can open.
        if buffer.status != DBSTATUS_S_OK.0 as u32 {
            continue;
        }
        if let Some(path) = decode(&buffer) {
            paths.push(path);
        }
    }

    let _ = rowset.ReleaseRows(
        obtained,
        hrows,
        std::ptr::null(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    CoTaskMemFree(Some(hrows.cast()));
    paths
}

fn decode(buffer: &RowBuf) -> Option<PathBuf> {
    let chars = (buffer.length / 2).min(PATH_CHARS);
    let value = &buffer.path[..chars];
    let end = value.iter().position(|&c| c == 0).unwrap_or(chars);
    path_from_url(&String::from_utf16_lossy(&value[..end]))
}

/// `System.ItemUrl` → a path we can open.
///
/// The index speaks URLs because it holds more than files — mail, contacts, and whatever else
/// a filter handler was installed for. Those have no path and are not ours to open, so
/// anything that is not `file:` is dropped rather than guessed at.
fn path_from_url(url: &str) -> Option<PathBuf> {
    // `file:C:/Users/me/report.pdf` — one colon, no slashes, which is the index's own spelling
    // rather than RFC 8089's. A UNC item comes through as `file://server/share/…`, and turning
    // every slash around gets `\\server\share\…` out of it for free.
    let path = url.strip_prefix("file:")?.replace('/', "\\");
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Take ownership of a string the provider allocated, and give its memory back.
unsafe fn take_string(raw: PWSTR) -> String {
    if raw.is_null() {
        return String::new();
    }
    let text = raw.to_string().unwrap_or_default();
    CoTaskMemFree(Some(raw.as_ptr().cast()));
    text
}

/// Confine the query to the folders the user chose to index — the same ones `funke-files`
/// walks, resolved the same way.
///
/// Not doing this would search every indexed location on the machine, which sounds like a
/// feature until the result list is the browser cache and the Windows folder. The clause is
/// appended to the generated `WHERE`, so it must open with `AND`.
fn scope_clause(roots: &[PathBuf]) -> String {
    let scopes: Vec<String> = roots
        .iter()
        .map(|root| {
            // Single quotes delimit the literal, so a single quote inside it is doubled.
            // Paths may legally contain one; SQL built from a path must survive it.
            let path = root.to_string_lossy().trim_end_matches('\\').replace('\'', "''");
            format!("SCOPE = 'file:{path}'")
        })
        .collect();
    if scopes.is_empty() {
        return String::new();
    }
    format!("AND ({})", scopes.join(" OR "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_is_scoped_to_the_indexed_roots() {
        assert_eq!(
            scope_clause(&[PathBuf::from(r"C:\Users\me\Documents")]),
            r"AND (SCOPE = 'file:C:\Users\me\Documents')"
        );

        // Several roots are alternatives, bracketed so the OR cannot spill into the terms
        // the query helper generated — that would make every root optional and the scope a
        // suggestion.
        assert_eq!(
            scope_clause(&[PathBuf::from(r"C:\work"), PathBuf::from(r"D:\archive\")]),
            r"AND (SCOPE = 'file:C:\work' OR SCOPE = 'file:D:\archive')"
        );
    }

    /// A folder is allowed to have an apostrophe in its name, and the SQL is built by hand.
    #[test]
    fn an_apostrophe_in_a_path_cannot_close_the_literal() {
        assert_eq!(
            scope_clause(&[PathBuf::from(r"C:\Ben's Files")]),
            r"AND (SCOPE = 'file:C:\Ben''s Files')"
        );
    }

    /// Never reachable through the provider — `resolve_index_roots` always yields at least the
    /// home directory — but an empty clause must mean "no restriction", not a broken `AND ()`.
    #[test]
    fn no_roots_means_no_clause() {
        assert_eq!(scope_clause(&[]), "");
    }

    /// The one test that proves the COM above is real rather than merely well-typed.
    ///
    /// Everything in this file compiles whether or not the dialect GUID is right, the binding
    /// offsets line up, or `GetNextRows`' fused count-and-out-pointer is being fed the way the
    /// provider expects — and every one of those failures looks the same from outside: no
    /// rows. So: ask the live index a question with a known answer.
    ///
    /// `--ignored` because it needs a running Windows Search service and an indexed home
    /// directory, neither of which a CI runner has. Run it on a real machine:
    /// `cargo test -p funke-content windows_search_on_this_machine -- --ignored --nocapture`
    #[test]
    #[ignore = "needs the Windows Search service and a populated index"]
    fn windows_search_on_this_machine() {
        let search = WinSearch::spawn();
        let home = dirs_home();
        // "the" is in the text of nearly every English document a person owns. If the index is
        // up and the plumbing is sound, this cannot come back empty.
        let hits = search.query("the", std::slice::from_ref(&home), 10).expect(
            "the Windows Search index refused the query — start the WSearch service, or read \
             the error the worker logged",
        );

        println!("{} hit(s) under {}", hits.len(), home.display());
        for path in &hits {
            println!("  {}", path.display());
        }
        assert!(
            !hits.is_empty(),
            "no content hits for a word every document contains — the query ran but the rows \
             did not survive the binding"
        );
        for path in &hits {
            assert!(
                path.starts_with(&home),
                "{} escaped the scope clause — the SCOPE restriction is not being applied",
                path.display()
            );
            assert!(path.is_file(), "{} is not a file we could open", path.display());
        }
    }

    fn dirs_home() -> PathBuf {
        PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE"))
    }

    /// What the index calls a location, and what the filesystem does.
    #[test]
    fn an_item_url_becomes_a_path_that_actually_exists() {
        assert_eq!(
            path_from_url("file:C:/Users/me/report.pdf"),
            Some(PathBuf::from(r"C:\Users\me\report.pdf"))
        );
        // UNC: the slashes turn around and the leading pair becomes the server prefix.
        assert_eq!(
            path_from_url("file://nas/share/notes.txt"),
            Some(PathBuf::from(r"\\nas\share\notes.txt"))
        );
        // The index holds things that are not files. They have no path, so they are not rows.
        assert_eq!(path_from_url("mapi://{S-1-5-21}/message"), None);
        assert_eq!(path_from_url("winrt://photos/1"), None);
        assert_eq!(path_from_url(""), None);
    }

    /// The provider writes into this struct; if its shape drifts from what the binding
    /// declares, rows come back as garbage rather than as an error.
    #[test]
    fn the_binding_describes_the_buffer_it_is_given() {
        let binding = path_binding();
        assert_eq!(binding.obStatus, std::mem::offset_of!(RowBuf, status));
        assert_eq!(binding.obLength, std::mem::offset_of!(RowBuf, length));
        assert_eq!(binding.obValue, std::mem::offset_of!(RowBuf, path));
        assert!(binding.obValue + binding.cbMaxLen <= std::mem::size_of::<RowBuf>());
    }

    /// The length the provider reports is the only thing standing between us and the rest of
    /// the buffer, and it arrives from another process.
    #[test]
    fn a_value_is_read_by_its_length_and_never_past_the_buffer() {
        let mut buffer = RowBuf {
            status: 0,
            length: 0,
            path: [0; PATH_CHARS],
        };
        let text: Vec<u16> = "file:C:/a/b.txt".encode_utf16().collect();
        buffer.path[..text.len()].copy_from_slice(&text);
        buffer.length = text.len() * 2; // OLE DB counts a WSTR in bytes.
        assert_eq!(decode(&buffer), Some(PathBuf::from(r"C:\a\b.txt")));

        // A nonsense length must not read past the buffer — the NUL still ends the value.
        buffer.length = usize::MAX;
        assert_eq!(decode(&buffer), Some(PathBuf::from(r"C:\a\b.txt")));

        // An empty value is not a path.
        buffer.length = 0;
        assert_eq!(decode(&buffer), None);
    }
}

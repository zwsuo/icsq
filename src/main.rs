//! icsq — 查询当前目录（或指定目录）中的 .ics 文件。
//!
//! 零依赖，仅使用标准库。支持字段选择、日期范围、多个关键字叠加搜索，
//! 以及 table / block / json / tsv 四种输出格式。

use std::cmp::Ordering;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = r#"icsq — 读取并查询 .ics 日历 / 日志 / 待办文件

用法:
  icsq [选项] [文件或目录...] [关键字...]

默认扫描当前目录下的 *.ics（不递归）。
位置参数如果是已存在的文件/目录，会作为输入；以 .ics 结尾但不存在则会报错；
其余位置参数作为关键字。多个关键字默认是 AND，加 --any 改为 OR。

选项:
  -d, --dir <PATH>         要扫描的目录，或单个 .ics 文件（可重复，默认 .）
  -F, --file <FILE>        指定单个 .ics 文件（可重复）
  -r, --recursive          递归扫描子目录
  -f, --field <FIELDS>     输出字段，逗号分隔，可重复；all = 全部
                           默认: dtstart,type,title,categories
      --all-fields         等价于 -f all
      --list-fields        列出支持的字段名

  -k, --keyword <TEXT>     关键字，可重复；位置参数同样作为关键字
      --any                多个关键字之间为 OR（默认 AND）
  -v, --invert-match       反向匹配
      --case-sensitive     区分大小写（默认忽略）
      --search-in <FIELDS> 在其中搜索，逗号分隔，默认:
                           summary,description,categories,location,uid
                           all = 所有属性值

      --from <DATE>        起始时间（含），别名 --since
      --to <DATE>          结束时间（含），别名 --until
      --on <DATE>          只取某一天/月/年
      --last <DUR>         取最近一段时间，如 7d、2w、6mo
      --time-field <FIELD> 日期过滤/排序所用字段（默认 DTSTART，
                           VTODO 回退到 DUE；也可指定 created 等）

  -t, --type <TYPE>        组件类型: event,journal,todo（可重复/逗号分隔）
      --status <S>         STATUS 过滤，如 final、confirmed
      --category <C>       CATEGORIES 过滤，可重复（任一命中）

      --sort <FIELD>       排序字段（默认 dtstart）
      --reverse            倒序
  -n, --limit <N>          最多输出 N 条
      --count              只输出匹配条数

  -o, --output <FMT>       table | block | json | tsv（默认 table）
      --full               等价于 --output block（保留换行、不截断）
      --table              强制 table 输出
      --json               等价于 --output json
      --tsv                等价于 --output tsv
      --no-header          不输出表头
      --max-width <N>      table 单元格自动换行宽度（默认 80）
      --truncate           不换行，超宽内容用 … 截断（单行表格）

  -h, --help               显示帮助
  -V, --version            显示版本

日期格式:
  2026-01-31              某天（--to 时含全天）
  2026-01                 某月
  2026                    某年
  2026-01-31T13:45        精确时间（也支持空格分隔）
  20260131                紧凑写法
  today / yesterday / tomorrow / now
  -7d / +1w               相对现在（d=天 w=周 mo=月 y=年 m=分钟 h=小时）

字段:
  type, title, summary, description, categories, dtstart, dtend, due,
  created, last-modified, dtstamp, status, class, url, location, geo,
  sequence, uid, attach, file
  也可以直接写任意 ICS 属性名，如 x-custom、rrule。

示例:
  icsq                                   # 列出当前目录全部
  icsq 会议.ics                          # 只读单个文件
  icsq a.ics b.ics -k 涵涵               # 读多个文件并搜索
  icsq -F 2026-01.ics --on 2026-01       # 指定文件 + 日期范围
  icsq --from 2026-01-01 --to 2026-01-31 # 当前目录一月份
  icsq --last 30d                        # 最近 30 天
  icsq -k AI -k 照片                     # 同时包含 AI 和 照片
  icsq 学校 涵涵 --any                   # 任一关键字
  icsq -f dtstart,summary,description --full
  icsq -f all --status final --tsv
  icsq --search-in all -k 理发 --json
"#;

// ---------------------------------------------------------------------------
// 命令行选项
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Output {
    Table,
    Block,
    Json,
    Tsv,
}

struct Opts {
    inputs: Vec<PathBuf>,
    recursive: bool,
    fields: Vec<String>,
    keywords: Vec<String>,
    any: bool,
    invert: bool,
    case_sensitive: bool,
    search_fields: Vec<String>,
    types: Vec<String>,
    statuses: Vec<String>,
    category_filters: Vec<String>,
    from: Option<i64>,
    to: Option<i64>,
    time_field: Option<String>,
    sort: String,
    reverse: bool,
    limit: Option<usize>,
    output: Output,
    full: bool,
    no_header: bool,
    max_width: usize,
    truncate: bool,
    count: bool,
    list_fields: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            recursive: false,
            fields: Vec::new(),
            keywords: Vec::new(),
            any: false,
            invert: false,
            case_sensitive: false,
            search_fields: ["summary", "description", "categories", "location", "uid"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            types: Vec::new(),
            statuses: Vec::new(),
            category_filters: Vec::new(),
            from: None,
            to: None,
            time_field: None,
            sort: "dtstart".to_string(),
            reverse: false,
            limit: None,
            output: Output::Table,
            full: false,
            no_header: false,
            max_width: 80,
            truncate: false,
            count: false,
            list_fields: false,
        }
    }
}

const DEFAULT_FIELDS: &[&str] = &["dtstart", "type", "title", "categories"];

const ALL_FIELDS: &[&str] = &[
    "uid",
    "type",
    "dtstart",
    "dtend",
    "due",
    "created",
    "last-modified",
    "dtstamp",
    "summary",
    "description",
    "categories",
    "location",
    "status",
    "class",
    "url",
    "geo",
    "sequence",
    "attach",
    "file",
];

fn main() {
    if let Err(e) = run() {
        eprintln!("icsq: {e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("icsq {VERSION}");
        return Ok(());
    }

    let mut opts = Opts::default();
    let mut i = 0usize;
    let mut positional_only = false;

    while i < args.len() {
        let arg = args[i].clone();

        if positional_only || !arg.starts_with('-') || arg == "-" {
            let path = Path::new(&arg);
            if path.exists() {
                opts.inputs.push(path.to_path_buf());
            } else if arg.to_ascii_lowercase().ends_with(".ics") {
                return Err(format!("文件不存在: {arg}"));
            } else {
                opts.keywords.push(arg);
            }
            i += 1;
            continue;
        }
        if arg == "--" {
            positional_only = true;
            i += 1;
            continue;
        }

        let (flag, inline) = match arg.find('=') {
            Some(pos) => (arg[..pos].to_string(), Some(arg[pos + 1..].to_string())),
            None => (arg.clone(), None),
        };

        match flag.as_str() {
            "-d" | "--dir" => {
                opts.inputs
                    .push(PathBuf::from(take(&args, &mut i, &inline, &flag)?));
            }
            "-F" | "--file" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                let path = PathBuf::from(&v);
                if !path.is_file() {
                    return Err(format!("--file: 不是有效文件: {v}"));
                }
                opts.inputs.push(path);
            }
            "-r" | "--recursive" => opts.recursive = true,
            "-f" | "--field" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                for f in split_list(&v) {
                    if f.eq_ignore_ascii_case("all") {
                        opts.fields.extend(ALL_FIELDS.iter().map(|s| s.to_string()));
                    } else {
                        opts.fields.push(normalize_field(&f));
                    }
                }
            }
            "--all-fields" => {
                opts.fields = ALL_FIELDS.iter().map(|s| s.to_string()).collect();
            }
            "--list-fields" => opts.list_fields = true,

            "-k" | "--keyword" => {
                opts.keywords.push(take(&args, &mut i, &inline, &flag)?);
            }
            "--any" => opts.any = true,
            "-v" | "--invert-match" => opts.invert = true,
            "--case-sensitive" => opts.case_sensitive = true,
            "--search-in" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                let list = split_list(&v);
                if list.iter().any(|s| s.eq_ignore_ascii_case("all")) {
                    opts.search_fields = vec!["*".to_string()];
                } else {
                    opts.search_fields = list.iter().map(|s| normalize_field(s)).collect();
                }
            }

            "--from" | "--since" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.from = Some(parse_user_date(&v, &flag)?.start_ts());
            }
            "--to" | "--until" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.to = Some(parse_user_date(&v, &flag)?.end_ts());
            }
            "--on" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                let b = parse_user_date(&v, &flag)?;
                opts.from = Some(b.start_ts());
                opts.to = Some(b.end_ts());
            }
            "--last" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                let secs =
                    parse_duration(&v).map_err(|e| format!("--last: {e}（示例: 7d, 2w, 6mo）"))?;
                let now = now_local().ts();
                opts.from = Some(now - secs);
                opts.to = Some(now);
            }
            "--time-field" => {
                opts.time_field = Some(normalize_field(&take(&args, &mut i, &inline, &flag)?));
            }

            "-t" | "--type" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.types
                    .extend(split_list(&v).iter().map(|s| normalize_type(s)));
            }
            "--status" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.statuses
                    .extend(split_list(&v).iter().map(|s| s.to_ascii_uppercase()));
            }
            "--category" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.category_filters.extend(split_list(&v));
            }

            "--sort" => {
                opts.sort = normalize_field(&take(&args, &mut i, &inline, &flag)?);
            }
            "--reverse" => opts.reverse = true,
            "-n" | "--limit" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.limit = Some(
                    v.parse::<usize>()
                        .map_err(|_| format!("--limit 需要正整数，收到: {v}"))?,
                );
            }
            "--count" => opts.count = true,

            "-o" | "--output" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.output = match v.to_ascii_lowercase().as_str() {
                    "table" => Output::Table,
                    "block" | "full" | "long" => Output::Block,
                    "json" => Output::Json,
                    "tsv" => Output::Tsv,
                    other => {
                        return Err(format!("未知输出格式 {other}（可选 table/block/json/tsv）"))
                    }
                };
            }
            "--full" => opts.full = true,
            "--table" => opts.output = Output::Table,
            "--json" => opts.output = Output::Json,
            "--tsv" => opts.output = Output::Tsv,
            "--no-header" => opts.no_header = true,
            "--truncate" => opts.truncate = true,
            "--max-width" => {
                let v = take(&args, &mut i, &inline, &flag)?;
                opts.max_width = v
                    .parse::<usize>()
                    .map_err(|_| format!("--max-width 需要正整数，收到: {v}"))?
                    .max(8);
            }

            other if other.starts_with('-') => {
                return Err(format!("未知参数 {other}（使用 --help 查看用法）"));
            }
            _ => {
                opts.keywords.push(arg);
            }
        }

        i += 1;
    }

    if opts.list_fields {
        print_field_list();
        return Ok(());
    }

    if opts.inputs.is_empty() {
        opts.inputs.push(PathBuf::from("."));
    }

    if opts.fields.is_empty() {
        opts.fields = DEFAULT_FIELDS.iter().map(|s| s.to_string()).collect();
    }
    dedup(&mut opts.fields);

    if opts.full && opts.output == Output::Table {
        opts.output = Output::Block;
    }

    if let (Some(from), Some(to)) = (opts.from, opts.to) {
        if from > to {
            return Err(format!(
                "--from 晚于 --to（from={}, to={}）",
                format_ts(from),
                format_ts(to)
            ));
        }
    }

    let files = collect_files(&opts.inputs, opts.recursive)?;
    if files.is_empty() {
        let where_desc = opts
            .inputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "没有找到 .ics 文件（输入: {where_desc}{}）",
            if opts.recursive { "，已递归" } else { "" }
        ));
    }

    let mut items: Vec<Item> = Vec::new();
    for entry in &files {
        let bytes = fs::read(&entry.path)
            .map_err(|e| format!("读取 {} 失败: {e}", entry.path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        items.extend(parse_ics(&text, &entry.label));
    }

    // 过滤
    let has_range = opts.from.is_some() || opts.to.is_some();
    items.retain(|item| {
        if !matches_type(item, &opts.types) {
            return false;
        }
        if !matches_status(item, &opts.statuses) {
            return false;
        }
        if !matches_category(item, &opts.category_filters) {
            return false;
        }
        if has_range {
            match item_ts(item, opts.time_field.as_deref()) {
                Some(ts) => {
                    if let Some(from) = opts.from {
                        if ts < from {
                            return false;
                        }
                    }
                    if let Some(to) = opts.to {
                        if ts > to {
                            return false;
                        }
                    }
                }
                None => return false,
            }
        }
        matches_keywords(item, &opts)
    });

    // 排序
    sort_items(&mut items, &opts);
    if opts.reverse {
        items.reverse();
    }
    if let Some(n) = opts.limit {
        items.truncate(n);
    }

    // 输出
    if opts.count {
        println!("{}", items.len());
        return Ok(());
    }

    match opts.output {
        Output::Table => print_table(&items, &opts),
        Output::Block => print_block(&items, &opts),
        Output::Json => print_json(&items, &opts),
        Output::Tsv => print_tsv(&items, &opts),
    }

    Ok(())
}

fn take(
    args: &[String],
    i: &mut usize,
    inline: &Option<String>,
    flag: &str,
) -> Result<String, String> {
    if let Some(v) = inline {
        return Ok(v.clone());
    }
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{flag} 需要一个参数"))
}

fn split_list(s: &str) -> Vec<String> {
    s.split(|c| c == ',' || c == ';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

fn dedup(v: &mut Vec<String>) {
    let mut out: Vec<String> = Vec::new();
    for x in v.drain(..) {
        if !out.iter().any(|y| y.eq_ignore_ascii_case(&x)) {
            out.push(x);
        }
    }
    *v = out;
}

fn normalize_field(s: &str) -> String {
    match s.trim().to_ascii_lowercase().as_str() {
        "date" | "start" | "begin" | "dtstart" => "dtstart",
        "end" | "finish" | "dtend" => "dtend",
        "deadline" | "due" => "due",
        "desc" | "body" | "content" | "note" | "notes" | "description" => "description",
        "tags" | "tag" | "category" | "categories" => "categories",
        "id" | "uid" => "uid",
        "name" | "subject" | "title" => "title",
        "kind" | "type" => "type",
        "filename" | "path" | "file" => "file",
        "modified" | "updated" | "lastmodified" | "last-modified" => "last-modified",
        "location" | "place" => "location",
        "attachment" | "attachments" | "attach" => "attach",
        other => other,
    }
    .to_string()
}

fn normalize_type(s: &str) -> String {
    match s.trim().to_ascii_lowercase().as_str() {
        "event" | "vevent" => "event",
        "journal" | "vjournal" | "note" => "journal",
        "todo" | "vtodo" | "task" => "todo",
        other => other,
    }
    .to_string()
}

fn print_field_list() {
    let rows = [
        ("file", "来源文件名"),
        ("type", "组件类型 event / journal / todo"),
        ("title", "SUMMARY；为空时取 DESCRIPTION 第一行"),
        ("summary", "SUMMARY 原始值"),
        ("description", "DESCRIPTION 全文"),
        ("categories", "CATEGORIES，多个用逗号连接"),
        ("dtstart", "开始时间（VJOURNAL/VEVENT）"),
        ("dtend", "结束时间"),
        ("due", "VTODO 截止时间"),
        ("created", "创建时间"),
        ("last-modified", "最后修改时间"),
        ("dtstamp", "DTSTAMP"),
        ("status", "STATUS"),
        ("class", "CLASS"),
        ("url", "URL"),
        ("location", "LOCATION"),
        ("geo", "GEO"),
        ("sequence", "SEQUENCE"),
        ("uid", "UID"),
        ("attach", "附件摘要（文件名、类型、大小），不输出 base64"),
    ];
    for (name, desc) in rows {
        println!("{name:<15} {desc}");
    }
    println!("\n任何其他属性名也可直接使用，例如 rrule、x-custom。");
}

// ---------------------------------------------------------------------------
// ICS 解析
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug)]
struct Item {
    kind: String,
    props: Vec<Prop>,
    file: String,
}

impl Item {
    fn prop(&self, name: &str) -> Option<&Prop> {
        self.props
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    fn text(&self, name: &str) -> Option<String> {
        self.prop(name).map(|p| unescape_text(&p.value))
    }

    fn text_values(&self, name: &str) -> Vec<String> {
        self.props
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| unescape_text(&p.value))
            .collect()
    }
}

fn parse_ics(content: &str, file: &str) -> Vec<Item> {
    let lines = unfold_lines(content);
    let mut stack: Vec<(String, Vec<Prop>)> = Vec::new();
    let mut items = Vec::new();

    for line in lines {
        if let Some(rest) = line.strip_prefix("BEGIN:") {
            stack.push((rest.trim().to_ascii_uppercase(), Vec::new()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("END:") {
            let kind = rest.trim().to_ascii_uppercase();
            if let Some((k, props)) = stack.pop() {
                if matches!(k.as_str(), "VEVENT" | "VTODO" | "VJOURNAL") {
                    items.push(Item {
                        kind: k,
                        props,
                        file: file.to_string(),
                    });
                } else if k != kind {
                    // 容错：忽略不严格配对的嵌套组件
                }
            }
            continue;
        }
        if let Some(prop) = parse_prop(&line) {
            if let Some((_, props)) = stack.last_mut() {
                props.push(prop);
            }
        }
    }

    items
}

/// RFC 5545 折行：以空格或制表符开头的行是上一行的延续。
fn unfold_lines(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in content.split_inclusive('\n') {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
            }
        } else if !line.is_empty() {
            out.push(line.to_string());
        }
    }
    out
}

fn parse_prop(line: &str) -> Option<Prop> {
    let colon = find_unquoted(line, ':')?;
    let head = &line[..colon];
    let value = line[colon + 1..].to_string();

    let mut segments = split_unquoted(head, ';');
    let name = segments.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }

    let mut params = Vec::new();
    for seg in segments {
        if let Some(eq) = find_unquoted(seg, '=') {
            let k = seg[..eq].trim().to_ascii_uppercase();
            let v = unquote(seg[eq + 1..].trim());
            params.push((k, v));
        }
    }

    Some(Prop {
        name,
        params,
        value,
    })
}

fn find_unquoted(s: &str, needle: char) -> Option<usize> {
    let mut quoted = false;
    for (idx, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == needle && !quoted {
            return Some(idx);
        }
    }
    None
}

fn split_unquoted(s: &str, sep: char) -> impl Iterator<Item = &str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut quoted = false;
    for (idx, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            parts.push(&s[start..idx]);
            start = idx + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts.into_iter()
}

/// 按分隔符切分，但忽略反斜杠转义后的分隔符（用于 CATEGORIES 中的 `\,`）。
fn split_escaped(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut escaped = false;
    for (idx, c) in s.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == sep {
            parts.push(&s[start..idx]);
            start = idx + c.len_utf8();
        }
    }
    parts.push(&s[start..]);
    parts
}

fn category_list(item: &Item) -> Vec<String> {
    let mut out = Vec::new();
    for prop in item
        .props
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("CATEGORIES"))
    {
        for part in split_escaped(&prop.value, ',') {
            let cat = unescape_text(part).trim().to_string();
            if !cat.is_empty() {
                out.push(cat);
            }
        }
    }
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// 解码 RFC 5545 TEXT 转义。
fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 日期与时间
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateTime {
    y: i64,
    mo: u32,
    d: u32,
    h: u32,
    mi: u32,
    s: u32,
    has_time: bool,
}

impl DateTime {
    fn ts(&self) -> i64 {
        days_from_civil(self.y, self.mo as i64, self.d as i64) * 86_400
            + (self.h as i64) * 3_600
            + (self.mi as i64) * 60
            + self.s as i64
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Prec {
    Year,
    Month,
    Day,
    Time,
}

#[derive(Clone, Copy, Debug)]
struct Bound {
    dt: DateTime,
    prec: Prec,
}

impl Bound {
    fn start_ts(&self) -> i64 {
        match self.prec {
            Prec::Year => DateTime {
                y: self.dt.y,
                mo: 1,
                d: 1,
                h: 0,
                mi: 0,
                s: 0,
                has_time: false,
            }
            .ts(),
            Prec::Month => DateTime {
                y: self.dt.y,
                mo: self.dt.mo,
                d: 1,
                h: 0,
                mi: 0,
                s: 0,
                has_time: false,
            }
            .ts(),
            Prec::Day => DateTime {
                y: self.dt.y,
                mo: self.dt.mo,
                d: self.dt.d,
                h: 0,
                mi: 0,
                s: 0,
                has_time: false,
            }
            .ts(),
            Prec::Time => self.dt.ts(),
        }
    }

    fn end_ts(&self) -> i64 {
        match self.prec {
            Prec::Year => DateTime {
                y: self.dt.y,
                mo: 12,
                d: 31,
                h: 23,
                mi: 59,
                s: 59,
                has_time: false,
            }
            .ts(),
            Prec::Month => DateTime {
                y: self.dt.y,
                mo: self.dt.mo,
                d: days_in_month(self.dt.y, self.dt.mo),
                h: 23,
                mi: 59,
                s: 59,
                has_time: false,
            }
            .ts(),
            Prec::Day => DateTime {
                y: self.dt.y,
                mo: self.dt.mo,
                d: self.dt.d,
                h: 23,
                mi: 59,
                s: 59,
                has_time: false,
            }
            .ts(),
            Prec::Time => self.dt.ts(),
        }
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
}

fn from_ts_utc(ts: i64) -> DateTime {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    DateTime {
        y,
        mo,
        d,
        h: (secs / 3_600) as u32,
        mi: ((secs % 3_600) / 60) as u32,
        s: (secs % 60) as u32,
        has_time: true,
    }
}

#[cfg(unix)]
mod sys {
    use super::{from_ts_utc, DateTime};

    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }

    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
        fn time(tloc: *mut i64) -> i64;
    }

    pub fn now_local() -> DateTime {
        let ts = unsafe { time(std::ptr::null_mut()) };
        local_from_ts(ts).unwrap_or_else(|| from_ts_utc(ts))
    }

    pub fn local_from_ts(ts: i64) -> Option<DateTime> {
        unsafe {
            let mut tm: Tm = std::mem::zeroed();
            if localtime_r(&ts, &mut tm).is_null() {
                return None;
            }
            Some(DateTime {
                y: tm.tm_year as i64 + 1900,
                mo: (tm.tm_mon + 1) as u32,
                d: tm.tm_mday as u32,
                h: tm.tm_hour as u32,
                mi: tm.tm_min as u32,
                s: tm.tm_sec as u32,
                has_time: true,
            })
        }
    }
}

#[cfg(not(unix))]
fn local_from_ts(ts: i64) -> Option<DateTime> {
    Some(from_ts_utc(ts))
}

#[cfg(unix)]
fn local_from_ts(ts: i64) -> Option<DateTime> {
    sys::local_from_ts(ts)
}

fn now_local() -> DateTime {
    #[cfg(unix)]
    {
        sys::now_local()
    }
    #[cfg(not(unix))]
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        from_ts_utc(ts)
    }
}

/// 解析 ICS 中的日期/时间属性。带 Z 的按 UTC 解析后转成本地时间；
/// 带 TZID 或浮动时间按字面墙上时间处理。
fn parse_ics_date(prop: &Prop) -> Option<DateTime> {
    let raw = prop.value.trim();
    if raw.is_empty() {
        return None;
    }
    let utc = raw.ends_with('Z') || raw.ends_with('z');
    let core = if utc { &raw[..raw.len() - 1] } else { raw };

    let split = core.find(|c| c == 'T' || c == 't' || c == ' ');
    let (dpart, tpart) = match split {
        Some(pos) => (&core[..pos], Some(&core[pos + 1..])),
        None => (core, None),
    };

    let (y, mo, d) = parse_ics_date_part(dpart)?;
    let (h, mi, s, has_time) = match tpart {
        Some(t) => {
            let (h, mi, s) = parse_ics_time_part(t)?;
            (h, mi, s, true)
        }
        None => (0, 0, 0, false),
    };

    if mo < 1 || mo > 12 || d < 1 || d > days_in_month(y, mo) {
        return None;
    }

    let dt = DateTime {
        y,
        mo,
        d,
        h,
        mi,
        s,
        has_time,
    };

    if utc {
        local_from_ts(dt.ts())
    } else {
        Some(dt)
    }
}

fn parse_ics_date_part(s: &str) -> Option<(i64, u32, u32)> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        let y = digits[0..4].parse().ok()?;
        let mo = digits[4..6].parse().ok()?;
        let d = digits[6..8].parse().ok()?;
        return Some((y, mo, d));
    }
    let cleaned = s.replace('/', "-");
    let parts: Vec<&str> = cleaned.split('-').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        3 => Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        )),
        _ => None,
    }
}

fn parse_ics_time_part(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim_end_matches('Z').trim_end_matches('z');
    let cleaned = s.replace(':', "").replace('.', "");
    let (h, mi, sec) = match s.matches(':').count() {
        2 => {
            let parts: Vec<&str> = s.split(':').collect();
            (
                parts[0].parse::<u32>().ok()?,
                parts[1].parse::<u32>().ok()?,
                parts[2].parse::<u32>().ok()?,
            )
        }
        1 => {
            let parts: Vec<&str> = s.split(':').collect();
            (
                parts[0].parse::<u32>().ok()?,
                parts[1].parse::<u32>().ok()?,
                0,
            )
        }
        _ if cleaned.len() >= 6 => (
            cleaned[0..2].parse::<u32>().ok()?,
            cleaned[2..4].parse::<u32>().ok()?,
            cleaned[4..6].parse::<u32>().ok()?,
        ),
        _ => return None,
    };
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some((h, mi, sec))
}

fn parse_user_date(s: &str, flag: &str) -> Result<Bound, String> {
    let raw = s.trim();
    let lower = raw.to_ascii_lowercase();

    let err = || format!("{flag}: 无法识别的日期 {raw:?}");

    match lower.as_str() {
        "now" => {
            let n = now_local();
            return Ok(Bound {
                dt: DateTime {
                    has_time: true,
                    ..n
                },
                prec: Prec::Time,
            });
        }
        "today" => {
            let n = now_local();
            return Ok(Bound {
                dt: DateTime {
                    y: n.y,
                    mo: n.mo,
                    d: n.d,
                    h: 0,
                    mi: 0,
                    s: 0,
                    has_time: false,
                },
                prec: Prec::Day,
            });
        }
        "yesterday" | "tomorrow" => {
            let n = now_local();
            let shift = if lower == "yesterday" {
                -86_400
            } else {
                86_400
            };
            let t = from_ts_utc(n.ts() + shift);
            return Ok(Bound {
                dt: DateTime {
                    y: t.y,
                    mo: t.mo,
                    d: t.d,
                    h: 0,
                    mi: 0,
                    s: 0,
                    has_time: false,
                },
                prec: Prec::Day,
            });
        }
        _ => {}
    }

    if let Some(sign) = lower.chars().next() {
        if (sign == '-' || sign == '+') && lower.len() > 1 {
            let secs = parse_duration(&lower[1..]).map_err(|_| err())?;
            let delta = if sign == '-' { -secs } else { secs };
            let t = now_local().ts() + delta;
            let dt = from_ts_utc(t);
            return Ok(Bound {
                dt: DateTime {
                    has_time: true,
                    ..dt
                },
                prec: Prec::Time,
            });
        }
    }

    let (date_part, time_part) = match raw.find(|c| c == 'T' || c == 't' || c == ' ') {
        Some(pos) => (&raw[..pos], Some(&raw[pos + 1..])),
        None => (raw, None),
    };

    // YYYY, YYYY-MM, YYYY-MM-DD
    let cleaned = date_part.replace('/', "-");
    let pieces: Vec<&str> = cleaned.split('-').filter(|p| !p.is_empty()).collect();

    let (y, mo, d, prec) = if pieces.len() == 1 {
        let y: i64 = pieces[0].parse().map_err(|_| err())?;
        if !(1..=9999).contains(&y) {
            return Err(err());
        }
        (y, 1, 1, Prec::Year)
    } else if pieces.len() == 2 {
        let y: i64 = pieces[0].parse().map_err(|_| err())?;
        let mo: u32 = pieces[1].parse().map_err(|_| err())?;
        if !(1..=9999).contains(&y) || !(1..=12).contains(&mo) {
            return Err(err());
        }
        (y, mo, 1, Prec::Month)
    } else if pieces.len() == 3 {
        let y: i64 = pieces[0].parse().map_err(|_| err())?;
        let mo: u32 = pieces[1].parse().map_err(|_| err())?;
        let d: u32 = pieces[2].parse().map_err(|_| err())?;
        if !(1..=9999).contains(&y) || !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo)
        {
            return Err(err());
        }
        (y, mo, d, Prec::Day)
    } else {
        return Err(err());
    };

    match time_part {
        Some(t) => {
            let (h, mi, sec) = parse_ics_time_part(t).ok_or_else(err)?;
            Ok(Bound {
                dt: DateTime {
                    y,
                    mo,
                    d,
                    h,
                    mi,
                    s: sec,
                    has_time: true,
                },
                prec: Prec::Time,
            })
        }
        None => Ok(Bound {
            dt: DateTime {
                y,
                mo,
                d,
                h: 0,
                mi: 0,
                s: 0,
                has_time: false,
            },
            prec,
        }),
    }
}

/// 7d / 2w / 6mo / 1y / 3h / 30m（m = 分钟，mo = 月）
fn parse_duration(s: &str) -> Result<i64, String> {
    let s = s.trim().to_ascii_lowercase();
    let idx = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    if idx == 0 {
        return Err(format!("缺少数量: {s}"));
    }
    let n: i64 = s[..idx].parse().map_err(|_| format!("无效数量: {s}"))?;
    let unit = &s[idx..];
    let secs = match unit {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
        "d" | "day" | "days" => 86_400,
        "w" | "week" | "weeks" => 604_800,
        "mo" | "month" | "months" => 2_592_000,
        "y" | "year" | "years" => 31_536_000,
        _ => return Err(format!("未知时间单位 {unit:?}")),
    };
    Ok(n * secs)
}

fn format_ts(ts: i64) -> String {
    let dt = from_ts_utc(ts);
    format_dt(&dt)
}

fn format_dt(dt: &DateTime) -> String {
    if !dt.has_time {
        format!("{:04}-{:02}-{:02}", dt.y, dt.mo, dt.d)
    } else if dt.s != 0 {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            dt.y, dt.mo, dt.d, dt.h, dt.mi, dt.s
        )
    } else {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            dt.y, dt.mo, dt.d, dt.h, dt.mi
        )
    }
}

// ---------------------------------------------------------------------------
// 过滤 / 排序
// ---------------------------------------------------------------------------

fn matches_type(item: &Item, types: &[String]) -> bool {
    if types.is_empty() {
        return true;
    }
    let kind = normalize_type(&item.kind);
    types.iter().any(|t| t == &kind)
}

fn matches_status(item: &Item, statuses: &[String]) -> bool {
    if statuses.is_empty() {
        return true;
    }
    match item.prop("STATUS") {
        Some(p) => {
            let v = p.value.trim().to_ascii_uppercase();
            statuses.iter().any(|s| s == &v)
        }
        None => false,
    }
}

fn matches_category(item: &Item, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    let cats: Vec<String> = category_list(item)
        .iter()
        .map(|s| s.trim().to_lowercase())
        .collect();
    filters
        .iter()
        .any(|f| cats.iter().any(|c| c == &f.to_lowercase()))
}

fn item_ts(item: &Item, time_field: Option<&str>) -> Option<i64> {
    if let Some(field) = time_field {
        return item
            .prop(&field.to_ascii_uppercase())
            .and_then(parse_ics_date)
            .map(|d| d.ts());
    }
    let names: &[&str] = match item.kind.as_str() {
        "VTODO" => &["DUE", "DTSTART", "CREATED"],
        _ => &["DTSTART", "DTEND", "CREATED"],
    };
    for name in names {
        if let Some(p) = item.prop(name) {
            if let Some(dt) = parse_ics_date(p) {
                return Some(dt.ts());
            }
        }
    }
    None
}

fn field_ts(item: &Item, field: &str) -> Option<i64> {
    item.prop(&field.to_ascii_uppercase())
        .and_then(parse_ics_date)
        .map(|d| d.ts())
}

fn build_haystack(item: &Item, fields: &[String], case_sensitive: bool) -> String {
    let mut parts: Vec<String> = Vec::new();

    if fields.iter().any(|f| f == "*") {
        for p in &item.props {
            if p.param("ENCODING")
                .is_some_and(|v| v.eq_ignore_ascii_case("BASE64"))
            {
                continue;
            }
            if p.value.len() > 200_000 {
                continue;
            }
            parts.push(unescape_text(&p.value));
        }
        parts.push(item.file.clone());
    } else {
        for f in fields {
            if f == "file" {
                parts.push(item.file.clone());
            } else {
                parts.extend(item.text_values(&f.to_ascii_uppercase()));
            }
        }
    }

    let joined = parts.join("\n");
    if case_sensitive {
        joined
    } else {
        joined.to_lowercase()
    }
}

fn matches_keywords(item: &Item, opts: &Opts) -> bool {
    if opts.keywords.is_empty() {
        return true;
    }
    let haystack = build_haystack(item, &opts.search_fields, opts.case_sensitive);

    let hit = |kw: &str| -> bool {
        let needle = if opts.case_sensitive {
            kw.to_string()
        } else {
            kw.to_lowercase()
        };
        haystack.contains(&needle)
    };

    let ok = if opts.any {
        opts.keywords.iter().any(|k| hit(k))
    } else {
        opts.keywords.iter().all(|k| hit(k))
    };

    if opts.invert {
        !ok
    } else {
        ok
    }
}

fn is_date_field(field: &str) -> bool {
    matches!(
        field,
        "dtstart"
            | "dtend"
            | "due"
            | "created"
            | "last-modified"
            | "dtstamp"
            | "completed"
            | "recurrence-id"
            | "exdate"
            | "rdate"
    )
}

fn sort_items(items: &mut [Item], opts: &Opts) {
    let date_sort = is_date_field(&opts.sort);
    items.sort_by(|a, b| {
        if date_sort {
            match (field_ts(a, &opts.sort), field_ts(b, &opts.sort)) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        } else {
            let sa = raw_field(a, &opts.sort).to_lowercase();
            let sb = raw_field(b, &opts.sort).to_lowercase();
            sa.cmp(&sb)
        }
    });
}

// ---------------------------------------------------------------------------
// 字段取值
// ---------------------------------------------------------------------------

fn raw_field(item: &Item, field: &str) -> String {
    match field {
        "type" => match item.kind.as_str() {
            "VEVENT" => "event".to_string(),
            "VTODO" => "todo".to_string(),
            "VJOURNAL" => "journal".to_string(),
            other => other.to_ascii_lowercase(),
        },
        "file" => item.file.clone(),
        "title" => {
            let summary = item.text("SUMMARY").unwrap_or_default();
            let s = summary.trim();
            if !s.is_empty() {
                return s.to_string();
            }
            item.text("DESCRIPTION")
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .to_string()
        }
        "summary" => item.text("SUMMARY").unwrap_or_default(),
        "description" => item.text("DESCRIPTION").unwrap_or_default(),
        "categories" => {
            let mut tags = category_list(item);
            tags.dedup();
            tags.join(", ")
        }
        "attach" => attach_summary(item),
        _ => {
            let name = field.to_ascii_uppercase();
            match item.prop(&name) {
                None => String::new(),
                Some(p) => {
                    if is_date_field(field) || name.ends_with("DATE") || name == "COMPLETED" {
                        parse_ics_date(p)
                            .map(|d| format_dt(&d))
                            .unwrap_or_else(|| unescape_text(&p.value))
                    } else {
                        unescape_text(&p.value)
                    }
                }
            }
        }
    }
}

fn attach_summary(item: &Item) -> String {
    let mut out = Vec::new();
    for p in item
        .props
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("ATTACH"))
    {
        let name = p
            .param("FILENAME")
            .or_else(|| p.param("X-LABEL"))
            .unwrap_or("attachment");
        if p.param("ENCODING")
            .is_some_and(|v| v.eq_ignore_ascii_case("BASE64"))
        {
            let est = p.value.len() * 3 / 4;
            out.push(format!("{name} (~{} KB, base64)", est.div_ceil(1024)));
        } else {
            out.push(format!("{name}: {}", p.value));
        }
    }
    out.join("; ")
}

// ---------------------------------------------------------------------------
// 输出
// ---------------------------------------------------------------------------

fn print_table(items: &[Item], opts: &Opts) {
    let headers: Vec<String> = opts.fields.iter().map(|f| header_name(f)).collect();
    let rows: Vec<Vec<Vec<String>>> = items
        .iter()
        .map(|item| {
            opts.fields
                .iter()
                .map(|f| {
                    let raw = raw_field(item, f);
                    if opts.truncate {
                        vec![sanitize_cell(&raw, opts.max_width)]
                    } else {
                        cell_lines(&raw, opts.max_width)
                    }
                })
                .collect()
        })
        .collect();

    let mut widths: Vec<usize> = headers.iter().map(|h| display_width(h)).collect();
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            for line in cell {
                widths[idx] = widths[idx].max(display_width(line));
            }
        }
    }

    if !opts.no_header {
        let last = headers.iter().rposition(|h| !h.is_empty()).unwrap_or(0);
        for (idx, h) in headers.iter().enumerate() {
            if idx < last {
                pad_print(h, widths[idx], false);
            } else if idx == last {
                print!("{h}");
            }
        }
        println!();
    }

    for row in &rows {
        let height = row.iter().map(|c| c.len()).max().unwrap_or(1);
        let last = row
            .iter()
            .rposition(|c| c.iter().any(|l| !l.is_empty()))
            .unwrap_or(0);
        for line_idx in 0..height {
            for (idx, cell) in row.iter().enumerate() {
                if idx > last {
                    break;
                }
                let text = cell.get(line_idx).map(|s| s.as_str()).unwrap_or("");
                if idx < last {
                    pad_print(text, widths[idx], false);
                } else {
                    print!("{text}");
                }
            }
            println!();
        }
    }
}

fn header_name(field: &str) -> String {
    match field {
        "dtstart" => "START".to_string(),
        "dtend" => "END".to_string(),
        "last-modified" => "MODIFIED".to_string(),
        "description" => "DESCRIPTION".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn pad_print(s: &str, width: usize, last: bool) {
    print!("{s}");
    if !last {
        let w = display_width(s);
        for _ in w..width {
            print!(" ");
        }
        print!("  ");
    }
}

/// 把单元格内容拆成多行：保留显式换行，超宽的行按显示宽度自动折行。
fn cell_lines(s: &str, max_width: usize) -> Vec<String> {
    let normalized = s.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim_matches('\n');
    let mut out: Vec<String> = Vec::new();

    for line in trimmed.split('\n') {
        let line = line.trim_end();
        let line = line.replace('\t', "    ");
        if line.trim().is_empty() {
            out.push(String::new());
        } else {
            out.extend(wrap_line(&line, max_width));
        }
    }

    // 连续空行折叠为一个，去掉行尾空行
    let mut squeezed: Vec<String> = Vec::new();
    for line in out {
        if line.is_empty() && squeezed.last().is_some_and(|l| l.is_empty()) {
            continue;
        }
        squeezed.push(line);
    }
    while squeezed.last().is_some_and(|l| l.is_empty()) {
        squeezed.pop();
    }
    if squeezed.is_empty() {
        squeezed.push(String::new());
    }
    squeezed
}

/// 按显示宽度折行；优先在空格处断开，中文等无空格文本则硬断。
fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
    if display_width(line) <= max_width {
        return vec![line.to_string()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;
    let mut last_space: Option<usize> = None;

    for c in line.chars() {
        let cw = char_width(c);
        if cur_w + cw > max_width && !cur.is_empty() && c != ' ' {
            if let Some(pos) = last_space {
                let head = cur[..pos].trim_end().to_string();
                let rest = cur[pos..].trim_start().to_string();
                if !head.is_empty() {
                    out.push(head);
                } else {
                    out.push(cur.trim_end().to_string());
                }
                cur = rest;
            } else {
                out.push(cur.trim_end().to_string());
                cur.clear();
            }
            cur_w = display_width(&cur);
            last_space = cur
                .char_indices()
                .rev()
                .find(|(_, ch)| *ch == ' ')
                .map(|(i, _)| i);

            // 断在空格处后仍可能放不下当前字符
            if cur_w + cw > max_width && !cur.is_empty() {
                out.push(cur.trim_end().to_string());
                cur.clear();
                cur_w = 0;
                last_space = None;
            }
        }

        cur.push(c);
        cur_w += cw;
        if c == ' ' {
            last_space = Some(cur.len() - 1);
        }
    }

    if !cur.trim().is_empty() {
        out.push(cur.trim_end().to_string());
    }
    out
}

fn sanitize_cell(s: &str, max_width: usize) -> String {
    let mut one_line = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars() {
        match c {
            '\r' => {}
            '\n' => {
                if !one_line.is_empty() {
                    pending_space = true;
                }
            }
            '\t' => {
                if !one_line.is_empty() {
                    pending_space = true;
                }
            }
            _ => {
                if pending_space {
                    if !one_line.ends_with(' ') && !one_line.ends_with('⏎') {
                        one_line.push(' ');
                    }
                    pending_space = false;
                }
                one_line.push(c);
            }
        }
    }
    let trimmed = one_line.trim();
    truncate_width(trimmed, max_width)
}

fn truncate_width(s: &str, max_width: usize) -> String {
    if display_width(s) <= max_width {
        return s.to_string();
    }
    let limit = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut width = 0usize;
    for c in s.chars() {
        let w = char_width(c);
        if width + w > limit {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push('…');
    out
}

fn char_width(c: char) -> usize {
    let u = c as u32;
    if c.is_control() {
        return 0;
    }
    // 组合符号
    if (0x0300..=0x036F).contains(&u) || (0x1AB0..=0x1AFF).contains(&u) {
        return 0;
    }
    let wide = (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0x303E).contains(&u)
        || (0x3041..=0x33FF).contains(&u)
        || (0x3400..=0x4DBF).contains(&u)
        || (0x4E00..=0x9FFF).contains(&u)
        || (0xA000..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE10..=0xFE19).contains(&u)
        || (0xFE30..=0xFE6F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1F64F).contains(&u)
        || (0x1F900..=0x1F9FF).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u);
    if wide {
        2
    } else {
        1
    }
}

fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn print_block(items: &[Item], opts: &Opts) {
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        println!("[{}] {}", idx + 1, item.file);
        for field in &opts.fields {
            let value = raw_field(item, field);
            let value = value.trim_matches(|c| c == '\n' || c == '\r');
            if value.trim().is_empty() {
                continue;
            }
            if value.contains('\n') {
                println!("    {}:", field);
                for line in value.lines() {
                    println!("      {}", line.trim_end());
                }
            } else {
                println!("    {}: {}", field, value);
            }
        }
    }
}

fn print_tsv(items: &[Item], opts: &Opts) {
    if !opts.no_header {
        println!(
            "{}",
            opts.fields
                .iter()
                .map(|f| f.to_lowercase())
                .collect::<Vec<_>>()
                .join("\t")
        );
    }
    for item in items {
        let row: Vec<String> = opts
            .fields
            .iter()
            .map(|f| escape_tsv(&raw_field(item, f)))
            .collect();
        println!("{}", row.join("\t"));
    }
}

fn escape_tsv(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "")
        .replace('\n', "\\n")
}

fn print_json(items: &[Item], opts: &Opts) {
    println!("[");
    for (idx, item) in items.iter().enumerate() {
        println!("  {{");
        for (fidx, field) in opts.fields.iter().enumerate() {
            let value = raw_field(item, field);
            println!(
                "    \"{}\": \"{}\"{}",
                json_escape(field),
                json_escape(&value),
                if fidx + 1 == opts.fields.len() {
                    ""
                } else {
                    ","
                }
            );
        }
        println!("  }}{}", if idx + 1 == items.len() { "" } else { "," });
    }
    println!("]");
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 文件扫描
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct InputFile {
    path: PathBuf,
    label: String,
}

fn collect_files(inputs: &[PathBuf], recursive: bool) -> Result<Vec<InputFile>, String> {
    let mut out: Vec<InputFile> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for input in inputs {
        let meta = fs::metadata(input).map_err(|e| format!("无法访问 {}: {e}", input.display()))?;

        if meta.is_dir() {
            let mut stack = vec![input.clone()];
            while let Some(dir) = stack.pop() {
                let entries = fs::read_dir(&dir)
                    .map_err(|e| format!("无法读取目录 {}: {e}", dir.display()))?;
                for entry in entries {
                    let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
                    let path = entry.path();
                    let file_type = entry
                        .file_type()
                        .map_err(|e| format!("读取文件类型失败 {}: {e}", path.display()))?;
                    if file_type.is_dir() {
                        if recursive {
                            let name = entry.file_name();
                            if !name.to_string_lossy().starts_with('.') {
                                stack.push(path);
                            }
                        }
                    } else if is_ics(&path) {
                        push_input(&mut out, &mut seen, &path, Some(input));
                    }
                }
            }
        } else if meta.is_file() {
            push_input(&mut out, &mut seen, input, None);
        } else {
            return Err(format!("不是普通文件或目录: {}", input.display()));
        }
    }

    out.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

fn is_ics(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ics"))
}

fn push_input(
    out: &mut Vec<InputFile>,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: &Path,
    base: Option<&Path>,
) {
    let key = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(key) {
        return;
    }
    let label = match base {
        Some(base) => path
            .strip_prefix(base)
            .ok()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file_label(path)),
        None => file_label(path),
    };
    out.push(InputFile {
        path: path.to_path_buf(),
        label,
    });
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfolds_folded_lines() {
        let text =
            "BEGIN:VCALENDAR\r\nDESCRIPTION:第一行\r\n 第二行\r\n\t第三行\r\nEND:VCALENDAR\r\n";
        let lines = unfold_lines(text);
        assert_eq!(
            lines,
            vec![
                "BEGIN:VCALENDAR",
                "DESCRIPTION:第一行第二行第三行",
                "END:VCALENDAR"
            ]
        );
    }

    #[test]
    fn unescapes_text() {
        assert_eq!(unescape_text(r"a\nb\,c\;d\\e"), "a\nb,c;d\\e");
    }

    #[test]
    fn splits_escaped_categories() {
        assert_eq!(split_escaped(r"a\,b,c", ','), vec![r"a\,b", "c"]);
    }

    #[test]
    fn parses_component_and_categories() {
        let text = "\
BEGIN:VCALENDAR
BEGIN:VJOURNAL
DTSTART:20260115T135314
SUMMARY:开始用成语了。
CATEGORIES:生活,喝酒
DESCRIPTION:第一行\\n第二行
END:VJOURNAL
END:VCALENDAR
";
        let items = parse_ics(text, "x.ics");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.kind, "VJOURNAL");
        assert_eq!(item.text("SUMMARY").unwrap(), "开始用成语了。");
        assert_eq!(category_list(item), vec!["生活", "喝酒"]);
        assert_eq!(
            parse_ics_date(item.prop("DTSTART").unwrap()).unwrap().ts(),
            DateTime {
                y: 2026,
                mo: 1,
                d: 15,
                h: 13,
                mi: 53,
                s: 14,
                has_time: true
            }
            .ts()
        );
    }

    #[test]
    fn parses_user_date_precision() {
        let year = parse_user_date("2026", "--to").unwrap();
        assert_eq!(format_ts(year.end_ts()), "2026-12-31 23:59:59");

        let month = parse_user_date("2026-02", "--to").unwrap();
        assert_eq!(format_ts(month.end_ts()), "2026-02-28 23:59:59");

        let day = parse_user_date("2026-02-28", "--from").unwrap();
        assert_eq!(format_ts(day.start_ts()), "2026-02-28 00:00");

        let exact = parse_user_date("2026-02-28 10:30", "--to").unwrap();
        assert_eq!(format_ts(exact.end_ts()), "2026-02-28 10:30");

        assert!(parse_user_date("2026-02-30", "--from").is_err());
        assert!(parse_user_date("not-a-date", "--from").is_err());
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("7d").unwrap(), 7 * 86_400);
        assert_eq!(parse_duration("2w").unwrap(), 14 * 86_400);
        assert_eq!(parse_duration("30m").unwrap(), 1_800);
        assert_eq!(parse_duration("6mo").unwrap(), 6 * 2_592_000);
        assert!(parse_duration("d").is_err());
        assert!(parse_duration("7x").is_err());
    }

    #[test]
    fn escapes_json() {
        assert_eq!(json_escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }

    #[test]
    fn parses_props_with_params() {
        let p =
            parse_prop("ATTACH;ENCODING=BASE64;FILENAME=\"a b.jpg\";VALUE=BINARY:AAAA").unwrap();
        assert_eq!(p.name, "ATTACH");
        assert_eq!(p.param("FILENAME"), Some("a b.jpg"));
        assert_eq!(p.value, "AAAA");
    }

    #[test]
    fn picks_title_fallback() {
        let text = "\
BEGIN:VCALENDAR
BEGIN:VJOURNAL
SUMMARY:
DESCRIPTION:第一行标题\\n正文
END:VJOURNAL
END:VCALENDAR
";
        let items = parse_ics(text, "x.ics");
        assert_eq!(items[0].text("DESCRIPTION").unwrap(), "第一行标题\n正文");
        assert_eq!(raw_field(&items[0], "title"), "第一行标题");
    }

    #[test]
    fn wraps_long_lines() {
        assert_eq!(wrap_line("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_line("ab cd ef", 5), vec!["ab cd", "ef"]);
        assert_eq!(wrap_line("中文测试", 4), vec!["中文", "测试"]);
        assert_eq!(wrap_line("短", 80), vec!["短"]);
        assert_eq!(
            cell_lines("第一段\n\n第二段", 80),
            vec!["第一段", "", "第二段"]
        );
    }

    #[test]
    fn collects_single_file_and_directory() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("icsq-test-{}-{}", std::process::id(), stamp));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.ics"), "x").unwrap();
        fs::write(dir.join("b.txt"), "x").unwrap();
        fs::write(dir.join("sub/c.ics"), "x").unwrap();

        let single = collect_files(&[dir.join("a.ics")], false).unwrap();
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].label, "a.ics");

        let flat = collect_files(&[dir.clone()], false).unwrap();
        assert_eq!(flat.len(), 1);

        let deep = collect_files(&[dir.clone()], true).unwrap();
        assert_eq!(deep.len(), 2);
        assert!(deep.iter().any(|f| f.label == "sub/c.ics"));

        fs::remove_dir_all(&dir).unwrap();
    }
}

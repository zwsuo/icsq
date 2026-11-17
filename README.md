# icsq

一个零依赖的 Rust 命令行工具，用来读取、筛选和搜索 `.ics` 文件：既可以扫描目录，也可以只读单个文件。

专为「一个目录下很多 `.ics`，想按字段/日期/关键字快速查询」的场景设计，
对 `VJOURNAL`（日记/笔记）、`VEVENT`、`VTODO` 都支持。本目录里的文件正是
jtx Board + DAVx5 生成的 `VJOURNAL` 集合。

## 编译与安装

```bash
cd icsq
cargo build --release        # 产物: target/release/icsq

# 可选：安装到 PATH
cargo install --path .
# 或
install -Dm755 target/release/icsq ~/.local/bin/icsq
```

没有第三方 crate，不需要联网拉依赖。

## 快速上手

在存放 `.ics` 的目录里运行：

```bash
icsq                                  # 默认列出：开始时间、类型、标题、分类
icsq --on 2026-01                     # 2026 年 1 月
icsq --from 2026-01-01 --to 2026-03-31
icsq --last 30d                       # 最近 30 天
icsq -k 涵涵 -k 数学                  # 同时包含两个关键字（AND）
icsq 学校 涵涵 --any                  # 任一关键字（OR）
icsq -k 理发 --search-in all          # 在所有属性里找
icsq -f dtstart,summary,description --full   # 看完整正文
icsq -f all --tsv > out.tsv           # 导出
icsq --count                          # 只输出条数
```

只读单个文件：

```bash
icsq 会议.ics                          # 位置参数：直接传文件
icsq -d 会议.ics                       # -d 也接受文件
icsq -F 会议.ics                       # 或用 --file 显式指定
icsq 会议.ics -k 议程 -f dtstart,title,description --full
icsq 会议.ics -f description           # 纵向显示，内容完整
icsq 会议.ics -f summary --raw         # 只输出值，不带字段名
icsq 会议.ics -f description --wrap    # 按 --max-width 自动换行
icsq 会议.ics -f description --wrap --max-width 40   # 指定换行宽度
icsq 会议.ics -f dtstart,title --wide  # 横向表格：一行一条记录
```

一次读多个文件，或文件与目录混用：

```bash
icsq jan.ics feb.ics --from 2026-01-01
icsq -d extra.ics -d ~/calendars --last 7d
```

如果不在此目录运行，用 `-d` 指定目录：

```bash
icsq -d ~/Downloads/af77c523-db64-4ae9-bed6-09ba340ebd6b --last 7d
```

### 输入规则

- 位置参数：**已存在的文件/目录**会作为输入；以 `.ics` 结尾但不存在会直接报错；其余位置参数才当作关键字。
- 想搜索一个看起来像文件名的关键字，用 `-k`，例如 `icsq -k foo.ics`。
- `-d/--dir` 和 `-F/--file` 都可以重复；同一个文件被多次指定会自动去重。
- 类型过滤、日期过滤、关键字、排序、输出格式等，对单文件和多文件完全一致。

## 常用参数

| 参数 | 说明 |
| --- | --- |
| `-d, --dir PATH` | 扫描目录，或读单个 `.ics` 文件；可重复，默认 `.` |
| `-F, --file FILE` | 显式指定单个 `.ics` 文件；可重复 |
| `-r, --recursive` | 递归子目录（跳过隐藏目录） |
| `-f, --field LIST` | 输出字段，逗号分隔，可重复；`all` 为全部 |
| `--list-fields` | 列出字段及其含义 |
| `-k, --keyword TEXT` | 关键字，可重复；位置参数也是关键字 |
| `--any` | 多关键字 OR（默认 AND） |
| `-v, --invert-match` | 反向匹配 |
| `--case-sensitive` | 区分大小写（默认忽略） |
| `--search-in LIST` | 搜索范围，默认 `summary,description,categories,location,uid`；`all` 为所有属性 |
| `--from / --to` | 日期范围，端点均包含（别名 `--since / --until`） |
| `--on DATE` | 某天/某月/某年 |
| `--last DUR` | 最近一段时间，如 `7d`、`2w`、`6mo` |
| `--time-field FIELD` | 用哪个时间字段过滤/排序，默认 DTSTART（VTODO 回退 DUE） |
| `-t, --type TYPE` | `event` / `journal` / `todo`，可重复 |
| `--status S` | 按 STATUS 过滤，如 `final` |
| `--category C` | 按 CATEGORIES 过滤，可重复（任一命中） |
| `--sort FIELD` / `--reverse` | 排序字段与倒序，默认 `dtstart` |
| `-n, --limit N` | 限制条数 |
| `--count` | 只输出匹配数量 |
| `-o, --output FMT` | `table` / `block` / `json` / `tsv` |
| `--full` | 输出所有非空字段的完整内容（`-f all` + block）；显式 `-f` 时只输出指定字段 |
| `--max-width N` | `--wrap` 的折行宽度，也是 `--truncate` 的上限，默认 80 |
| `--wrap` | 超宽值按 `--max-width` 自动换行，保留正文段落 |
| `--truncate` | 超宽值用 `…` 截断成一行（与 `--wrap` 互斥） |
| `--wide` | 横向表格：字段作列、一行一条记录（默认是纵向） |
| `--raw` | 只输出字段值，不输出字段名、序号和文件名（别名 `--values-only`） |
| `--no-header` | 不输出字段名 |

## 字段

默认字段：`dtstart,type,title,categories`。

- `title`：`SUMMARY`；为空时自动取 `DESCRIPTION` 第一行（本目录很多条目摘要为空，靠这个显示）
- `description`：`DESCRIPTION` 全文
- `categories`：`CATEGORIES`，多个用逗号连接
- `attach`：附件摘要（文件名、类型、估算大小），不会输出 base64
- `file`：来源文件名
- `type`：`event` / `journal` / `todo`
- 其余：`dtstart,dtend,due,completed,created,last-modified,dtstamp,status,priority,percent-complete,class,url,location,geo,sequence,duration,rrule,organizer,attendee`
- 也可以直接写任意属性名，如 `rrule`、`x-custom`

`-f all` 展开为上面所有常用字段。

## 日期写法

| 写法 | 含义 |
| --- | --- |
| `2026-01-31` | 某天（作为 `--to` 时包含当天 23:59:59） |
| `2026-01` | 某月 |
| `2026` | 某年 |
| `2026-01-31T13:45` | 精确时间，也支持空格分隔 |
| `20260131` | 紧凑写法 |
| `today` / `yesterday` / `tomorrow` / `now` | 相对今天 |
| `-7d` / `+1w` | 相对现在（`d` 天、`w` 周、`mo` 月、`y` 年、`m` 分钟、`h` 小时） |

## 关键字规则

- 多个 `-k` 或位置参数默认 **AND**：每个关键字都要在搜索范围内出现。
- `--any` 改为 **OR**。
- 搜索是大小写不敏感的子串匹配（可用 `--case-sensitive` 关闭）。
- `--search-in all` 会搜所有属性值（自动跳过 base64 附件和超大字段），并包含文件名。

## 输出

- `table`：默认**纵向布局**，每条记录一块，字段一行一个，字段名左对齐。内容完整不截断，正文里的换行和制表符折叠为空格；多条记录之间用空行分隔，并显示 `[序号] 文件名`。加 `--wide` 可切回字段作列的横向表格；加 `--wrap` 按 `--max-width` 自动换行；加 `--truncate` 用 `…` 截断超宽值。
- `--raw`：只输出字段值，每行一个，不带字段名、序号和文件名；多条记录之间空一行。适合 `icsq -f description --raw | ...` 这类管道处理。
- `block`（`--full`）：每条一块，保留正文换行；`--full` 未配合 `-f` 时会输出所有非空字段（`-f all`），适合快速查看一条记录的全部信息。
- `json`：对象数组，字段名为 key，适合再用 `jq` 处理。
- `tsv`：制表符分隔，换行转义为 `\n`，适合导入表格或脚本。

示例：

```bash
icsq --on 2026-01 -f dtstart,title,categories,uid --json | jq .
icsq --last 90d -f dtstart,title,categories --tsv > recent.tsv
```

## 已知的现成工具

如果只是偶尔看看，下列工具也能用，但各有取舍：

| 工具 | 说明 | 对本目录的限制 |
| --- | --- | --- |
| [`khal`](https://github.com/pimutils/khal) | Arch 仓库有：`pacman -S khal`。最强的终端日历，支持 `khal list START END` 和 `khal search`，能读 vdir 目录里的事件/待办 | **不支持 VJOURNAL**；本目录 46 个条目全是 VJOURNAL，`khal` 会当作不存在。且需要配置 vdir |
| Python [`icalendar`](https://pypi.org/project/icalendar/) | `pip install icalendar`，可直接遍历 `VJOURNAL`，适合写脚本 | 需要 Python 依赖；日期/关键字筛选、输出格式都要自己写 |
| Python `ics.py` | 语法简单，`Calendar(...)` 读文件 | 主要面向 VEVENT，VJOURNAL 支持有限 |
| `calcurse` | `calcurse -i file.ics` 可导入，TUI 浏览 | 导入式、交互式，不适合批量查询/管道 |
| `vdirsyncer` | 只负责同步 CalDAV/vdir | 不查询 |
| `gcalcli` | Google Calendar 查询 | 只针对 Google，本地 `.ics` 不行 |
| `grep` / `awk` | 临时救急：`grep -l 关键词 *.ics` | 不会展开折行、不处理转义、不能按日期筛选 |

结论：本目录是「一个 Radicale 集合 + 全是 VJOURNAL」的结构，现成工具里 `khal`
最接近但会直接忽略这些条目，所以写一个专用小工具更省事。若不想编译，可以先用：

```bash
python3 - <<'PY'
from icalendar import Calendar
from pathlib import Path
for p in Path('.').glob('*.ics'):
    for c in Calendar.from_ical(p.read_bytes()).walk():
        if c.name == 'VJOURNAL':
            print(c.get('DTSTART').dt, c.get('SUMMARY', ''))
PY
```

## 实现说明与限制

- 纯标准库实现，单文件 `src/main.rs`，无外部依赖。
- 正确处理 RFC 5545 的折行、TEXT 转义（`\n`、`\,`、`\;`、`\\`）、带参数的属性。
- 带 `Z` 的时间会按 UTC 解析后换算成本地时间显示；带 `TZID` 的按字面墙上时间处理（不读取时区库）。
- **不展开 RRULE 重复规则**，只按 `DTSTART` 的原始值列出；需要看规则可 `-f rrule`。
- 按时间过滤时，没有对应时间字段的条目会被排除。
- 递归时跳过以 `.` 开头的目录（例如 `.Radicale.cache`）。

## 测试

```bash
cargo test
```

## License

MIT © 2026 zwsuo

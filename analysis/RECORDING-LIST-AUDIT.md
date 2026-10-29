# 连接成功但录像列表为空：代码与原 APK 对照

分析日期：2026-10-27。用户实测现象：能连接记录仪，录像列表一直为空，没有报错。

后续状态：本报告记录的是修复前的对照结论。上述缺陷已在 0.1.3 修正，当前代码与本文中的“当前实现”描述有所不同；修复内容和验证结果见项目 README 的 0.1.3 节。尚未通过实车验收。

## 结论与边界

目前最贴合现象的代码缺陷是：列表中的录像路径被校验器拒绝后，解析器静默丢弃该条目，再把结果作为正常空列表返回。已使用当前项目的解析逻辑和锁定的 quick-xml 0.38.4 复现。原版明确处理的 `/./` 路径片段在复刻版中会触发这一问题。

这是已复现的缺陷，但还不能认定为本车此次故障的唯一根因：尚无记录仪实际列表响应。若实际响应确实包含 `/mnt/mmc/./Normal/…`，便能直接解释当前现象。

另外确认了分页策略、文件下载路径构建和 XML 字符引用处理的缺口。分析仅新增本文，没有修改应用逻辑，也没有生成新安装包。

## 原包身份与适配方案

- 用户提供：`/Users/coderleishuo/Downloads/vivo办公套件/base.apk`。
- SHA-256：`0e737970e57c16dce6a70eac673d089d99609aebc98bdd8846175bd16c4e4371`，与 `analysis/metadata.json` 完全一致，因此已有反汇编可用于本次核对。
- 原包车型资源 `assets/vehicle_names.json` 将“26星越L”归入 A7。
- A7 列表查询属性为普通 `DCIM`、紧急 `EVENT`。当前 `src-tauri/src/lib.rs:117` 已与其一致。
- 查询属性与 HTTP 文件目录是两套名称：`DCIM → Normal`，`EVENT → Event`。原版枚举同时保存 property 和 directory，不能混为一谈。

证据：`analysis/app-disassembly.txt:20659`（分类初始化）、`:20695`（字段赋值）、`:21023`（列表 URL）。

## 1. 最贴合“空列表且无报错”的缺陷：路径被静默过滤

当前处理链：

```text
parse_listing_page
  → 读出 <file><name>…</name></file>
  → media_path(name)
  → 校验失败：不加入 videos，不保留失败原因
  → entries 仍增加
  → entries 小于请求 count：list_videos 返回成功，warning = None
```

代码位置：

- `src-tauri/src/protocol.rs:94`：任一目录片段为 `.` 或 `..` 都拒绝。
- `src-tauri/src/protocol.rs:208`：`if let Ok(path) = media_path(&name)`，失败被直接忽略。
- `src-tauri/src/lib.rs:163`：空页或短页正常结束，不检查“有原始条目但全部过滤”的情况。
- `src/App.tsx:114`：零录像仅显示短暂的普通提示；普通提示在 4.5 秒后消失。

原版 `DvrProtocol$Video.<init>` 在验证前循环把 `/./` 替换为 `/`，再检查 A7 路径与分类和目录层级。它接受的路径形式包括裸文件名和 `/mnt/mmc/Normal/文件名.mp4`。

证据：`analysis/app-disassembly.txt:21602`，指令 `0019–0027` 为 `/./` 规范化；后续 A7 分支校验 `/mnt/mmc/{directory}/`。

复现输入（构造样例，并非实车抓包）：

```xml
<list><file>
  <name>/mnt/mmc/./Normal/20261027_082640.mp4</name>
  <size>100</size>
</file></list>
```

当前解析结果：原始条目数 `1`，录像数 `0`；第一页请求 count 为 `50`，上层会正常返回空列表。原版会先去除 `/./`，此样例通过其路径校验。

建议：规范化无害的 `.` 片段，继续拒绝 `..`；区分非视频条目和有效视频路径被拒绝；记录过滤数量与原因。“有视频条目但全部无法解析”应返回明确诊断。

## 2. 已复现：XML 字符引用被漏读

当前只处理 `Event::Text` 和 `Event::CData`，未处理 quick-xml 的 `Event::GeneralRef`。Text 分支中的 unescape 无法补回独立事件中已被忽略的字符。

构造样例：`<name>20261027_082640&#46;mp4</name>`。

XML 中 `&#46;` 表示句点。当前解析得到的名称缺少句点，扩展名校验失败，结果也是原始条目 `1`、录像 `0`。`&amp;` 等引用还可能损坏路径内容，导致后续下载失败。

原版使用 SAX 解析，字符引用经解析器展开后进入 characters 回调。本车是否使用字符引用尚不清楚，不能把它当作已确认的实车触发条件。

建议：正确拼接字符引用，覆盖数字引用和 XML 预定义实体；未知实体应明确报错。

## 3. 分页兼容策略与原版不一致

| 项目 | 原版 A7 | 当前实现 |
| --- | --- | --- |
| 初始分页 | 优先记住的值，否则 50000 | 50 |
| 外层回退候选 | 50000、128、100、32、31、8 | 50、20、1 |
| 首次 XML 失败后的策略 | 换候选，从头按新策略读取 | 同一起点换 count |
| 后续页 HTTP 500 | 原位置逐次减半，单条失败再确认 | 固定尝试 50、20、1 |
| 成功参数记忆 | 按设备与分类记录 | 无 |
| 响应上限/超时 | 16 MiB / 12 秒 | 8 MiB / 10 秒 |

原版回退数组已从用户 APK 的 DEX array-data 直接读取，确认为 `[50000, 128, 100, 32, 31, 8]`，并非根据字符串推测。

证据：`A7Listing.read`（反汇编第 199 行）、`A7Listing.readAll`（316）、`DvrClient.list`（19199）、`DvrClient.lambda$list$0`（18693）、`RecorderCompatibility.listingPageSize`（59083）。当前实现见 `src-tauri/src/lib.rs:130`。

这会影响部分固件的兼容性。但仅凭候选值不同，不能证明当前固件拒绝 50、20 或 1。若表现为始终无错误，静默过滤比纯 HTTP/解析失败更贴合现象。

## 4. 文件下载路径缺少 A7 转换

原版 `Video.httpPath(false)` 使用：

```text
普通录像：http://设备地址/mnt/mmc/Normal/经过编码的文件名.mp4
紧急录像：http://设备地址/mnt/mmc/Event/经过编码的文件名.mp4
```

当前解析器对裸文件名 `20261027_082640.mp4` 只补一个前导 `/`，下载层直接请求该路径，变成 `http://设备地址/20261027_082640.mp4`。

证据：原版 `analysis/app-disassembly.txt:21873`（httpPath），当前 `src-tauri/src/protocol.rs:104` 与 `src-tauri/src/lib.rs:230` 附近（download_video 中构建 URL）。

这是列表修复后仍可能遇到的独立下载缺陷，本身不能解释列表为什么为空。若设备返回的已经是完整正确路径，当前下载不一定受此问题影响。

## 5. “已连接”与诊断目前无法证明录像读取成功

- `src-tauri/src/connection.rs:248`：识别只请求 `action=get` 的 `DVR` 或 `Camera.Menu.FWversion`；未验证 `action=dir`。
- `src-tauri/src/protocol.rs:6`：响应包含 `OK` 且不是 HTML 即接受，与原版宽松探测思路接近；不能由此推出存储卡列表接口可用。
- `src/App.tsx:108`：普通和紧急列表并发请求；原版 `MainActivity.lambda$load$23`（反汇编 46895）按普通、紧急顺序读取。弱设备的并发兼容性值得核对，但当前无实车证据表明它造成失败。
- `list_videos` 没有写入 ConnectionState 的诊断。因此“复制连接诊断”看不到列表 URL、原始条目数、过滤条目数、分页结果和解析失败原因。
- 解析器还会把 `<response><status>unsupported</status></response>` 这样的完整但非列表 XML 当作空列表接受。这是诊断盲点；原版 SAX 实现同样没有严格限定列表根名，不把它列为二者差异。

## 验证结果

为避免缺失的 Tauri 依赖影响协议复现，临时建立独立 Rust 程序：复制当前 `protocol.rs`，仅移除 serde 导入、序列化 derive 和属性，保持解析及路径逻辑不变。使用 Cargo.lock 指定的 quick-xml 0.38.4，下载归档 SHA-256 与 lock 校验和一致；memchr 使用本机缓存 2.8.3。

- 协议文件现有 9 项单元测试全部通过，说明现有样例未覆盖这些缺口。
- 额外 6 组构造样例输出：

```text
canonical:         raw=1, videos=1, path=/mnt/mmc/Normal/20261027_082640.mp4
dot_segment:       raw=1, videos=0
bare_filename:     raw=1, videos=1, path=/20261027_082640.mp4
unknown_document:  raw=0, videos=0
escaped_extension: raw=1, videos=0
empty_list:        raw=0, videos=0
```

完整项目的 `cargo test --offline` 未能运行：本机缺少 lock 指定的 webpki-roots 1.0.11。没有将独立协议测试描述成整项目通过。没有连接车辆，也没有验证原版应用的实际请求结果。

## 下一步顺序

1. 增加列表诊断：分类、from/count、HTTP 状态、响应大小、原始条目数、有效录像数、过滤原因及数量。需要排查路径时仅取脱敏样例。
2. 修正 `/./` 规范化、XML 字符引用处理、A7 文件路径构建，并增加上述失败样例的回归测试。
3. 对齐原版分页回退策略，普通与紧急列表先按顺序请求，保留已成功读到的数据。
4. 实车取得一页列表响应，核对到底是设备返回空页，还是本地解析后清空。若响应包含上述 `/./` 形式，即可确认第一项为本次直接原因；否则继续按诊断检查分页和响应结构。

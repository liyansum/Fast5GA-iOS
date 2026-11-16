//! Fixed D73/Unicom port of fastboot/restore.py. No target file readback.
use super::*;
use idevice::afc::{errors::AfcError, FileInfo};
use idevice::services::core_device::{AppServiceClient, ProcessToken};
use idevice::RsdService;
use sha2::Sha384;
use std::{collections::BTreeMap, io::Read, path::Path, sync::Mutex, time::Duration};

const BUNDLE: &str = "/var/mobile/Library/Carrier Bundles/iPhone/Unicom_cn.bundle";
const OVERLAY: &str = "/var/mobile/Library/Carrier Bundles/Overlay";
const COMM: &str = "/System/Library/Frameworks/CoreTelephony.framework/Support/CommCenter";
const IPCC: &str = "Unicom_cn_73.0_original.ipcc";
static RUN_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Install,
    Directory,
    Write(usize),
    Reload,
}
const JOBS: [(&str, &str, u16); 8] = [
    ("unicom.der.pri", "overrides_D73_D74_D27_D28.der.pri", 0o644),
    (
        "unicom_overrides.plist",
        "overrides_D73_D74_D27_D28.plist",
        0o644,
    ),
    (
        "unicom_signature.plist",
        "signatures/overrides_D73_D74_D27_D28.plist",
        0o644,
    ),
    ("unicom_carrier.plist", "carrier.plist", 0o600),
    (
        "unicom_common_signature.plist",
        "signatures/common.plist",
        0o600,
    ),
    ("unicom_Info.plist", "Info.plist", 0o600),
    (
        "unicom_72.0.1.plist",
        "device+carrier+46001+D73+72.0.1.plist",
        0o555,
    ),
    (
        "unicom_73.0.plist",
        "device+carrier+46001+D73+73.0.plist",
        0o555,
    ),
];
fn plan(mode: &str) -> Result<Vec<Step>, String> {
    use Step::*;
    Ok(match mode {
        "check" => vec![],
        "full" => vec![
            Install,
            Write(0),
            Write(1),
            Write(2),
            Write(3),
            Write(4),
            Write(5),
            Write(6),
            Write(7),
            Reload,
            Write(6),
            Write(7),
        ],
        "normal" => vec![Write(6), Write(7), Reload, Write(6), Write(7)],
        "debug_ipcc" => vec![Install],
        "debug_bundle" => vec![Directory, Write(0), Write(1), Write(2)],
        "debug_aux" => vec![Directory, Write(3), Write(4), Write(5)],
        "debug_overlay" => vec![Write(6), Write(7)],
        "debug_reload" => vec![Reload],
        _ => return Err("未知操作".into()),
    })
}

#[derive(serde::Deserialize)]
struct HashEntry {
    size: usize,
    sha384: String,
}
fn payloads(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let manifest: BTreeMap<String, HashEntry> =
        serde_json::from_slice(&std::fs::read(dir.join("payloads.json")).map_err(err)?)
            .map_err(err)?;
    let mut files = BTreeMap::new();
    for name in JOBS.iter().map(|j| j.0).chain(std::iter::once(IPCC)) {
        let bytes = std::fs::read(dir.join(name)).map_err(err)?;
        let h = manifest
            .get(name)
            .ok_or_else(|| format!("缺少载荷清单：{name}"))?;
        if bytes.len() != h.size || hex::encode(Sha384::digest(&bytes)) != h.sha384 {
            return Err(format!("载荷校验失败：{name}"));
        }
        files.insert(name.into(), bytes);
    }
    // Manifest itself cannot silently substitute an unrelated carrier package.
    if hex::encode(Sha256::digest(&files[IPCC]))
        != "54b8895eec8e815830d6587d6500fdd11a34c6ed4522b4490e74801435f845c6"
    {
        return Err("原版 IPCC 不匹配".into());
    }
    if hex::encode(Sha384::digest(&files["unicom.der.pri"])) != "6bec2675e57b65327b396c0306f80e60e434552c4de52aed79174b0cd58ecdc8199a223b726e90610b3a7383ec4d7524" { return Err("15 Pro PRI 载荷不匹配".into()); }
    for name in ["unicom_72.0.1.plist", "unicom_73.0.plist"] {
        let p = plist::Value::from_reader(io::Cursor::new(&files[name])).map_err(err)?;
        let p = p.as_dictionary().ok_or("Overlay 格式不符")?;
        let traversal = "../".repeat(6)
            + &format!(
                "{}/overrides_D73_D74_D27_D28.der.pri",
                BUNDLE.trim_start_matches('/')
            );
        if p.get("DerPriFileName").and_then(|v| v.as_string()) != Some(traversal.as_str())
            || p.get("DerPriFileVersion").and_then(|v| v.as_string()) != Some("6.0.161")
            || p.get("DataIndicatorOverrideForNRMmwave")
                .and_then(|v| v.as_string())
                != Some("NRCA")
            || p.get("Show5GStandaloneSwitch").and_then(|v| v.as_boolean()) != Some(true)
            || p.get("ShowVoNRSwitch").and_then(|v| v.as_boolean()) != Some(true)
            || p.contains_key("DataIndicatorOverrideForLTE")
        {
            return Err(format!("Overlay 参数不匹配：{name}"));
        }
    }
    Ok(files)
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Stored ZIP with *both* Unix external attributes and Apple 0x5A53 mode.
/// Handwritten headers avoid zip crate masking S_IFLNK / S_IFREG bits.
fn archive(target: &str, payload: &[u8], permissions: u16) -> Result<Vec<u8>, String> {
    let tail = target.strip_prefix('/').ok_or("目标必须为绝对路径")?;
    if tail
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("目标目录异常".into());
    }
    let mut meta = Vec::new();
    let mut d = plist::Dictionary::new();
    d.insert("Version".into(), 2i64.into());
    plist::to_writer_binary(&mut meta, &plist::Value::Dictionary(d)).map_err(err)?;
    let mut entries: Vec<(String, u16, Vec<u8>)> = vec![
        ("META-INF/".into(), 0o40755, vec![]),
        (
            "META-INF/com.apple.ZipMetadata.plist".into(),
            0o100600,
            meta,
        ),
    ];
    for p in ["p0/", "p0/p1/", "p0/p1/p2/"] {
        entries.push((p.into(), 0o40755, vec![]));
    }
    entries.push((
        "p0/p1/p2/link".into(),
        0o120777,
        format!("../../../{tail}").into_bytes(),
    ));
    let mut p = String::new();
    for c in tail.split('/') {
        p.push_str(c);
        p.push('/');
        entries.push((p.clone(), 0o40755, vec![]));
    }
    entries.push(("payload".into(), 0o100000 | permissions, payload.to_vec()));
    let mut out = Vec::new();
    let mut central = Vec::new();
    fn u16v(b: &mut Vec<u8>, n: u16) {
        b.extend(n.to_le_bytes())
    }
    fn u32v(b: &mut Vec<u8>, n: u32) {
        b.extend(n.to_le_bytes())
    }
    for (name, mode, bytes) in &entries {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(bytes);
        let size = bytes.len() as u32;
        let n = name.len() as u16;
        u32v(&mut out, 0x04034b50);
        for v in [20, 0x800, 0, 0, 0x5d21] {
            u16v(&mut out, v)
        }
        for v in [crc, size, size] {
            u32v(&mut out, v)
        }
        u16v(&mut out, n);
        u16v(&mut out, 6);
        out.extend(name.as_bytes());
        for v in [0x5a53, 2, *mode] {
            u16v(&mut out, v)
        }
        out.extend(bytes);
        u32v(&mut central, 0x02014b50);
        for v in [0x0314, 20, 0x800, 0, 0, 0x5d21] {
            u16v(&mut central, v)
        }
        for v in [crc, size, size] {
            u32v(&mut central, v)
        }
        for v in [n, 6, 0, 0, 0] {
            u16v(&mut central, v)
        }
        u32v(&mut central, (*mode as u32) << 16);
        u32v(&mut central, offset);
        central.extend(name.as_bytes());
        for v in [0x5a53, 2, *mode] {
            u16v(&mut central, v)
        }
    }
    let offset = out.len() as u32;
    let length = central.len() as u32;
    out.extend(central);
    u32v(&mut out, 0x06054b50);
    for v in [0, 0, entries.len() as u16, entries.len() as u16] {
        u16v(&mut out, v)
    }
    u32v(&mut out, length);
    u32v(&mut out, offset);
    u16v(&mut out, 0);
    Ok(out)
}

async fn info(afc: &mut AfcClient, p: &str) -> Result<Option<FileInfo>, String> {
    match afc.get_file_info(p).await {
        Ok(i) => Ok(Some(i)),
        Err(idevice::IdeviceError::Afc(AfcError::ObjectNotFound | AfcError::ReadError)) => Ok(None),
        Err(e) => Err(format!("AFC stat {p}: {e}")),
    }
}
async fn mkdir(afc: &mut AfcClient, p: &str) -> Result<(), String> {
    let mut part = String::new();
    for c in p.split('/') {
        if c.is_empty() || c == "." || c == ".." {
            return Err("暂存路径异常".into());
        }
        if !part.is_empty() {
            part.push('/')
        }
        part.push_str(c);
        match info(afc, &part).await? {
            Some(i) if i.st_ifmt == "S_IFDIR" && i.st_link_target.is_none() => {}
            Some(_) => return Err(format!("暂存目录不是普通目录：{part}")),
            None => afc.mk_dir(&part).await.map_err(err)?,
        }
    }
    Ok(())
}
async fn read_media(afc: &mut AfcClient, p: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(i) = info(afc, p).await? else {
        return Ok(None);
    };
    if i.st_ifmt != "S_IFREG" || i.st_link_target.is_some() {
        return Err(format!("Books 配置类型异常：{p}"));
    }
    let mut f = afc.open(p, AfcFopenMode::RdOnly).await.map_err(err)?;
    let v = f.read_entire().await.map_err(err)?;
    f.close().await.map_err(err)?;
    Ok(Some(v))
}
async fn put(afc: &mut AfcClient, p: &str, bytes: &[u8]) -> Result<(), String> {
    if let Some(i) = info(afc, p).await? {
        if i.st_ifmt != "S_IFREG" || i.st_link_target.is_some() {
            return Err(format!("拒绝覆盖链接：{p}"));
        }
    }
    let mut f = afc.open(p, AfcFopenMode::WrOnly).await.map_err(err)?;
    f.write_entire(bytes).await.map_err(err)?;
    f.close().await.map_err(err)
}
/// Only explicit Media roots, unlink symlinks once, never follow them recursively.
async fn clean(afc: &mut AfcClient, root: &str) -> Result<(), String> {
    if !(root.starts_with("airlift-src-")
        || root.starts_with("airlift-link-")
        || root.starts_with("PublicStaging/Fast5GA_")
        || root.starts_with("UnicomFastRestore/"))
        || root
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("清理根目录不合法".into());
    }
    let parts: Vec<_> = root.split('/').collect();
    for n in 1..parts.len() {
        let p = parts[..n].join("/");
        if let Some(i) = info(afc, &p).await? {
            if i.st_ifmt != "S_IFDIR" || i.st_link_target.is_some() {
                return Err("清理父级存在链接".into());
            }
        } else {
            return Ok(());
        }
    }
    let mut stack = vec![(root.to_string(), false)];
    let mut count = 0;
    while let Some((p, visited)) = stack.pop() {
        count += 1;
        if count > 4096 {
            return Err("暂存清理条目过多".into());
        }
        let Some(i) = info(afc, &p).await? else {
            continue;
        };
        if i.st_ifmt == "S_IFDIR" && i.st_link_target.is_none() && !visited {
            stack.push((p.clone(), true));
            for name in afc.list_dir(&p).await.map_err(err)? {
                if name == "." || name == ".." {
                    continue;
                }
                if name.contains('/') || name.contains('\\') {
                    return Err("清理条目异常".into());
                }
                stack.push((format!("{p}/{name}"), false));
            }
        } else {
            afc.remove(&p).await.map_err(err)?;
        }
    }
    Ok(())
}

async fn stage(
    t: &mut AppDeviceTunnel,
    afc: &mut AfcClient,
    source: &str,
    ids: &[String],
    target: &str,
    bytes: &[u8],
    mode: u16,
    l: &Logger,
) -> Result<(), String> {
    let zip = archive(target, bytes, mode)?;
    let mut stream = t
        .connect_service("com.apple.streaming_zip_conduit", l)
        .await?;
    let mut d = plist::Dictionary::new();
    d.insert("MediaSubdir".into(), source.into());
    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &plist::Value::Dictionary(d)).map_err(err)?;
    stream
        .write_all(&(buf.len() as u32).to_be_bytes())
        .await
        .map_err(err)?;
    stream.write_all(&buf).await.map_err(err)?;
    stream.write_all(&zip).await.map_err(err)?;
    stream.flush().await.map_err(err)?;
    // Some releases close or omit the reply; the staged payload stat is authoritative.
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        let n = stream.read_u32().await?;
        if n > 1024 * 1024 {
            return Err(io::Error::other("zip response too large"));
        }
        let mut b = vec![0; n as usize];
        stream.read_exact(&mut b).await?;
        Ok::<_, io::Error>(())
    })
    .await;
    drop(stream);
    let i = info(afc, &format!("{source}/payload"))
        .await?
        .ok_or("StreamingZip 未生成载荷")?;
    if i.size != bytes.len() || i.st_ifmt != "S_IFREG" {
        return Err("暂存载荷大小或类型不符".into());
    }
    mkdir(afc, "Books/Sync").await?;
    put(
        afc,
        "Books/Sync/Books.plist",
        &build_books_plist(ids).map_err(err)?,
    )
    .await
}

async fn next_atc(
    stream: &mut Box<dyn ReadWrite>,
    session: i64,
) -> Result<plist::Dictionary, String> {
    loop {
        let d = tokio::time::timeout(Duration::from_secs(15), read_atc_dict(stream))
            .await
            .map_err(|_| "AirTraffic 响应超时")??;
        match atc_message_name(&d).as_deref() {
            Some("Ping") => send_atc_dict(stream, &make_atc_msg("Pong", session, None)).await?,
            Some("SyncFailed") => return Err("AirTraffic 拒绝同步（SyncFailed）".into()),
            _ => return Ok(d),
        }
    }
}
async fn sync(
    t: &mut AppDeviceTunnel,
    ids: &[String],
    dests: &[String],
    l: &Logger,
) -> Result<(), String> {
    let mut stream = t.connect_service("com.apple.atc", l).await?;
    let mut allowed = false;
    for _ in 0..20 {
        let d = next_atc(&mut stream, 0).await?;
        if atc_message_name(&d).as_deref() == Some("SyncAllowed") {
            allowed = true;
            break;
        }
    }
    if !allowed {
        return Err("未收到 SyncAllowed".into());
    }
    let mut host = plist::Dictionary::new();
    for (k, v) in [
        ("Type", "iTunes"),
        ("Version", "13.7.0.161"),
        ("MacOSVersion", "15.0"),
        ("SyncHostName", "Fast5GA"),
    ] {
        host.insert(k.into(), v.into());
    }
    host.insert("LibraryID".into(), random_hex(16).into());
    host.insert("Wakeable".into(), false.into());
    for k in ["SyncedDataclasses", "SyncedAssetTypes"] {
        host.insert(k.into(), plist::Value::Array(vec!["Book".into()]));
    }
    // Exact known-good token retained from the uploaded Windows implementation.
    let token=hex::decode("01012ba6a01f2ccf66a02613d5b72e0bc916004058a001a6874d18b5bd7b3395e25d79fa3ffcc67e718106d485c51540b828d1620e9f94f582d3bcc6f97e9088c923095ad8d36ab568fb45df61e286d25354b04c").map_err(err)?;
    host.insert("Grappa".into(), plist::Value::Data(token.clone()));
    let mut params = plist::Dictionary::new();
    params.insert("HostInfo".into(), host.clone().into());
    params.insert("LocalCloudSupport".into(), false.into());
    send_atc_dict(&mut stream, &make_atc_msg("HostInfo", 0, Some(params))).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut req = plist::Dictionary::new();
    req.insert(
        "Dataclasses".into(),
        plist::Value::Array(vec!["Book".into()]),
    );
    req.insert("DataclassAnchors".into(), plist::Dictionary::new().into());
    req.insert("HostInfo".into(), host.into());
    req.insert("Grappa".into(), plist::Value::Data(token));
    send_atc_dict(&mut stream, &make_atc_msg("RequestingSync", 1, Some(req))).await?;
    let mut ready = false;
    for _ in 0..24 {
        let d = next_atc(&mut stream, 1).await?;
        if atc_message_name(&d).as_deref() == Some("ReadyForSync") {
            ready = true;
            break;
        }
    }
    if !ready {
        return Err("未收到 ReadyForSync".into());
    }
    let mut types = plist::Dictionary::new();
    types.insert("Book".into(), 1i64.into());
    let mut meta = plist::Dictionary::new();
    meta.insert("SyncTypes".into(), types.into());
    meta.insert("DataclassAnchors".into(), plist::Dictionary::new().into());
    send_atc_dict(
        &mut stream,
        &make_atc_msg("FinishedSyncingMetadata", 1, Some(meta)),
    )
    .await?;
    let mut manifest = None;
    for _ in 0..20 {
        let d = next_atc(&mut stream, 1).await?;
        match atc_message_name(&d).as_deref() {
            Some("AssetManifest") => {
                manifest = Some(d);
                break;
            }
            Some("SyncFinished") => break,
            _ => {}
        }
    }
    let manifest = manifest.ok_or("未收到 AssetManifest")?;
    let books = manifest
        .get("Params")
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("AssetManifest"))
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("Book"))
        .and_then(|v| v.as_array())
        .ok_or("AssetManifest 缺少 Book")?;
    for id in ids {
        if !books.iter().any(|v| {
            v.as_dictionary().is_some_and(|d| {
                d.get("AssetID").and_then(|v| v.as_string()) == Some(id.as_str())
                    && d.get("IsDownload").and_then(|v| v.as_boolean()) == Some(true)
            })
        }) {
            return Err(format!("载荷不在同步清单：{id}"));
        }
    }
    for (idx, (id, dest)) in ids.iter().zip(dests).enumerate() {
        let mut p = plist::Dictionary::new();
        p.insert("AssetID".into(), id.clone().into());
        p.insert("Dataclass".into(), "Book".into());
        p.insert("AssetPath".into(), dest.clone().into());
        send_atc_dict(&mut stream, &make_atc_msg("FileComplete", 1, Some(p))).await?;
        if idx + 1 < ids.len() {
            tokio::time::sleep(Duration::from_millis(900)).await;
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}

async fn transaction(
    t: &mut AppDeviceTunnel,
    afc: &mut AfcClient,
    path: Option<&str>,
    payload: &[u8],
    mode: u16,
    l: &Logger,
) -> Result<(), String> {
    let nonce = random_hex(10);
    let source = format!("airlift-src-{nonce}");
    let link = format!("airlift-link-{nonce}");
    let backup = format!("UnicomFastRestore/{nonce}");
    let old = format!("{backup}/original");
    let kept = format!("{old}.kept");
    let mut saved = Vec::new();
    for p in ["Books/Books.plist", "Books/Sync/Books.plist"] {
        saved.push((p, read_media(afc, p).await?));
    }
    let outcome = tokio::time::timeout(Duration::from_secs(120), async {
        let target = if let Some(p) = path {
            p.rsplit_once('/').ok_or("目标路径异常")?.0
        } else {
            "/var/mobile/Library/Carrier Bundles"
        };
        let mut ids = Vec::new();
        let mut dests = Vec::new();
        if let Some(p) = path {
            mkdir(afc, &backup).await?;
            ids.push(posix_rel(p, AIRLOCK_ROOT));
            dests.push(old.clone());
        }
        ids.push(format!("../../{source}/p0/p1/p2/link"));
        dests.push(link.clone());
        if let Some(p) = path {
            ids.push(format!("../../{source}/payload"));
            dests.push(format!("{link}/{}", p.rsplit('/').next().unwrap()));
        }
        stage(t, afc, &source, &ids, target, payload, mode, l).await?;
        sync(t, &ids, &dests, l).await?;
        if path.is_some() {
            if info(afc, &format!("{source}/payload")).await?.is_some() {
                return Err("暂存载荷未移入目标".into());
            }
        } else {
            for rel in [
                "iPhone/Unicom_cn.bundle",
                "iPhone/Unicom_cn.bundle/signatures",
            ] {
                let i = info(afc, &format!("{link}/{rel}"))
                    .await?
                    .ok_or("/var 联通目录缺失，请先加载原版 IPCC")?;
                if i.st_ifmt != "S_IFDIR" || i.st_link_target.is_some() {
                    return Err("联通目录不是普通目录".into());
                }
            }
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "写入超时；已停止后续步骤".to_string())
    .and_then(|v| v);
    let mut cleanup = Vec::new();
    if outcome.is_err() && path.is_some() {
        match info(afc, &old).await {
            Ok(Some(_)) => match afc.rename(&old, &kept).await {
                Ok(()) => l.log(format!("保留旧文件：/var/mobile/Media/{kept}")),
                Err(e) => cleanup.push(format!("保留旧文件失败：{e}")),
            },
            Ok(None) => {}
            Err(e) => cleanup.push(e),
        }
    }
    for p in [&link, &source] {
        if let Err(e) = clean(afc, p).await {
            cleanup.push(e)
        }
    }
    if outcome.is_ok() && path.is_some() {
        if let Err(e) = clean(afc, &backup).await {
            cleanup.push(e)
        }
    }
    for (p, bytes) in saved {
        let r = async {
            if let Some(bytes) = bytes {
                mkdir(afc, "Books/Sync").await?;
                put(afc, p, &bytes).await
            } else if info(afc, p).await?.is_some() {
                afc.remove(p).await.map_err(err)
            } else {
                Ok(())
            }
        }
        .await;
        if let Err(e) = r {
            cleanup.push(e)
        }
    }
    if !cleanup.is_empty() {
        return Err(format!(
            "操作结果 {:?}；清理未完成：{}",
            outcome,
            cleanup.join("；")
        ));
    }
    outcome
}

async fn install(
    t: &mut AppDeviceTunnel,
    afc: &mut AfcClient,
    bytes: &[u8],
    l: &Logger,
) -> Result<(), String> {
    let root = format!("PublicStaging/Fast5GA_{}.ipcc", random_hex(10));
    let result = async {
        mkdir(afc, &root).await?;
        let mut zip = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(err)?;
        for n in 0..zip.len() {
            let mut entry = zip.by_index(n).map_err(err)?;
            let name = entry.name().to_string();
            if !name.starts_with("Payload/Unicom_cn.bundle/") && name != "Payload/" {
                return Err("原版 IPCC 包含异常路径".into());
            }
            if entry.enclosed_name().is_none() {
                return Err("IPCC 路径异常".into());
            }
            let p = format!("{root}/{name}");
            if name.ends_with('/') {
                mkdir(afc, p.trim_end_matches('/')).await?;
            } else {
                mkdir(afc, p.rsplit_once('/').unwrap().0).await?;
                let mut b = Vec::new();
                entry.read_to_end(&mut b).map_err(err)?;
                put(afc, &p, &b).await?;
            }
        }
        let mut proxy = t.connect_installation_proxy(l).await?;
        let mut options = plist::Dictionary::new();
        options.insert("PackageType".into(), "CarrierBundle".into());
        tokio::time::timeout(
            Duration::from_secs(120),
            proxy.install(format!("/{root}"), Some(options.into())),
        )
        .await
        .map_err(|_| "IPCC 安装超时")?
        .map_err(err)
    }
    .await;
    let cleanup = clean(afc, &root).await;
    result?;
    cleanup?;
    transaction(t, afc, None, &[], 0o644, l).await
}

fn exact_pid(processes: &[ProcessToken]) -> Result<Option<u32>, String> {
    let mut matches = Vec::new();
    for p in processes {
        let Some(u) = &p.executable_url else { continue };
        let raw = u
            .relative
            .strip_prefix("file://localhost")
            .or_else(|| u.relative.strip_prefix("file://"))
            .unwrap_or(&u.relative);
        if raw != COMM {
            continue;
        }
        if p.pid <= 1 {
            return Err("CommCenter PID 异常".into());
        }
        matches.push(p.pid);
    }
    if matches.len() > 1 {
        return Err("CommCenter 匹配不唯一".into());
    }
    Ok(matches.first().copied())
}
async fn processes(
    service: &mut AppServiceClient<Box<dyn ReadWrite>>,
) -> Result<Option<u32>, String> {
    let p = tokio::time::timeout(Duration::from_secs(10), service.list_processes())
        .await
        .map_err(|_| "进程查询超时")?
        .map_err(err)?;
    exact_pid(&p)
}
async fn reload(
    service: &mut AppServiceClient<Box<dyn ReadWrite>>,
    l: &Logger,
) -> Result<(), String> {
    if processes(service).await?.is_none() {
        return Err("未找到 CommCenter，未结束任何进程".into());
    }
    l.log("等待 5 秒后重载 CommCenter");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let old = processes(service)
        .await?
        .ok_or("CommCenter 状态已变化，停止操作")?;
    l.log(format!("仅结束一次 CommCenter（PID {old}）"));
    tokio::time::timeout(Duration::from_secs(10), service.send_signal(old, 9))
        .await
        .map_err(|_| "信号请求超时；不会重试")?
        .map_err(err)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    let mut stable: Option<(u32, tokio::time::Instant)> = None;
    while tokio::time::Instant::now() < deadline {
        let current = processes(service).await?;
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match (current, stable) {
            (Some(pid), Some((prior, since))) if pid != old && pid == prior => {
                if now.duration_since(since) >= Duration::from_secs(15) {
                    l.log(format!("新进程 {pid} 已持续运行 15 秒"));
                    return Ok(());
                }
            }
            (Some(pid), _) if pid != old => {
                stable = Some((pid, now));
                l.log(format!("观察新进程 {pid}"));
            }
            _ => stable = None,
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("90 秒内未确认新进程持续运行 15 秒；停止后续步骤，不再次结束进程".into())
}

async fn run(mode: String, pairing: String, dir: String, l: Logger) -> Result<String, String> {
    let steps = plan(&mode)?;
    let files = payloads(Path::new(&dir))?;
    l.log("载荷校验通过");
    let bytes = std::fs::read(&pairing).map_err(err)?;
    let mut t = connect_tunnel(&bytes, &l).await?;
    // Read-only identity check through the authenticated channel, before any Media changes.
    let mut ld = match &mut t {
        AppDeviceTunnel::Rsd { adapter, handshake } => handshake
            .connect::<LockdownClient>(adapter)
            .await
            .map_err(err)?,
        _ => return Err("需要本机 Remote Pairing 配对文件".into()),
    };
    let model = ld.get_value(Some("ProductType"), None).await.map_err(err)?;
    let version = ld
        .get_value(Some("ProductVersion"), None)
        .await
        .map_err(err)?;
    if model.as_string() != Some("iPhone15,2") || version.as_string() != Some("27.0.1") {
        return Err(format!(
            "载荷仅适用 iPhone 14 Pro / iOS 27.0.1，当前 {model:?} / {version:?}"
        ));
    }
    l.log("已连接 iPhone 14 Pro · iOS 27.0.1");
    // Preflight reload capability before either normal/full mode mutates files.
    let mut app = if mode == "check" || steps.contains(&Step::Reload) {
        let AppDeviceTunnel::Rsd { adapter, handshake } = &mut t else {
            unreachable!()
        };
        let s = handshake
            .services
            .get(AppServiceClient::<Box<dyn ReadWrite>>::rsd_service_name().as_ref())
            .ok_or("CoreDevice AppService 不可用；请确认开发者模式和开发服务")?;
        let stream = adapter.connect(s.port).await.map_err(err)?;
        let mut service = AppServiceClient::new(Box::new(stream) as Box<dyn ReadWrite>)
            .await
            .map_err(err)?;
        if processes(&mut service).await?.is_none() {
            return Err("开发服务未找到 CommCenter；尚未写入".into());
        }
        Some(service)
    } else {
        None
    };
    let mut afc = t.connect_afc(&l).await?;
    let active = afc
        .list_dir("/")
        .await
        .map_err(err)?
        .into_iter()
        .any(|n| n.starts_with("airlift-src-") || n.starts_with("airlift-link-"));
    if active {
        return Err("发现遗留或其他 AirLift 暂存项，请先处理后再执行；本次未写入".into());
    }
    for (idx, step) in steps.iter().enumerate() {
        let label = match step {
            Step::Install => "加载原版 73.0 IPCC 并检查目录".to_string(),
            Step::Directory => "检查 /var 联通目录".into(),
            Step::Reload => "重载 CommCenter".into(),
            Step::Write(i) => format!("写入 {}（{:04o}）", JOBS[*i].0, JOBS[*i].2),
        };
        l.log(format!("STEP {}/{} {label}", idx + 1, steps.len()));
        match step {
            Step::Install => install(&mut t, &mut afc, &files[IPCC], &l).await?,
            Step::Directory => transaction(&mut t, &mut afc, None, &[], 0o644, &l).await?,
            Step::Reload => reload(app.as_mut().unwrap(), &l).await?,
            Step::Write(i) => {
                let (name, leaf, mode) = JOBS[*i];
                let target = format!("{}/{leaf}", if *i < 6 { BUNDLE } else { OVERLAY });
                transaction(&mut t, &mut afc, Some(&target), &files[name], mode, &l).await?;
            }
        }
    }
    Ok(serde_json::json!({"ok":true,"mode":mode,"model":"iPhone15,2","ios":"27.0.1","steps":steps.len(),"verification":"staged payload moved; no target readback or destination mode measurement"}).to_string())
}

#[no_mangle]
pub unsafe extern "C" fn fg_run(
    mode: *const c_char,
    pairing: *const c_char,
    dir: *const c_char,
    host: *const c_char,
    port: u16,
    cb: ALLogCallback,
    ctx: *mut c_void,
    out: *mut *mut c_char,
) -> i32 {
    if out.is_null() {
        return 2;
    }
    *out = std::ptr::null_mut();
    let Ok(_lock) = RUN_LOCK.try_lock() else {
        *out = cstr("已有操作正在执行");
        return 1;
    };
    let mode = opt_str(mode, "");
    let pairing = opt_str(pairing, "");
    let dir = opt_str(dir, "");
    let host = opt_str(host, "10.7.0.1");
    if host.parse::<std::net::Ipv4Addr>().is_err() {
        *out = cstr("Device IP 必须为 IPv4 地址");
        return 1;
    }
    if port == 0 {
        *out = cstr("服务端口不能为 0");
        return 1;
    }
    clear_target_hosts();
    set_target_host(Some(host));
    set_target_port(port);
    let logger = Logger { cb, ctx };
    let result = crate::ffi_util::run_with_large_stack("Fast5GA", move || {
        idevice_ffi::run_sync_local(async {
            tokio::time::timeout(Duration::from_secs(1800), run(mode, pairing, dir, logger))
                .await
                .map_err(|_| "总操作超时".to_string())
                .and_then(|r| r)
        })
    });
    match result {
        Ok(Ok(json)) => {
            *out = cstr(json);
            0
        }
        Ok(Err(e)) | Err(e) => {
            *out = cstr(e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menus_match_batch() {
        assert_eq!(
            plan("normal").unwrap(),
            vec![
                Step::Write(6),
                Step::Write(7),
                Step::Reload,
                Step::Write(6),
                Step::Write(7)
            ]
        );
        assert_eq!(plan("full").unwrap().len(), 12);
        assert_eq!(plan("debug_reload").unwrap(), vec![Step::Reload]);
        assert_eq!(
            plan("debug_bundle").unwrap(),
            vec![
                Step::Directory,
                Step::Write(0),
                Step::Write(1),
                Step::Write(2)
            ]
        );
        assert!(plan("unexpected").is_err());
    }
    #[test]
    fn unix_and_apple_modes_survive() {
        for mode in [0o555, 0o600, 0o644] {
            let z = archive(BUNDLE, b"hello", mode).unwrap();
            let mut a = zip::ZipArchive::new(io::Cursor::new(&z)).unwrap();
            let mut f = a.by_name("payload").unwrap();
            assert_eq!(f.unix_mode(), Some(0o100000 | mode as u32));
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            assert_eq!(b, b"hello");
            drop(f);
            assert_eq!(
                a.by_name("p0/p1/p2/link").unwrap().unix_mode(),
                Some(0o120777)
            );
            assert!(z.windows(6).filter(|w|*w==[0x53,0x5a,2,0,mode as u8,((0o100000|mode)>>8)as u8]).count()>=2);
        }
    }
    #[test]
    fn exact_commcenter_only() {
        let p = |pid, path: &str| ProcessToken {
            pid,
            executable_url: Some(idevice::services::core_device::ExecutableUrl {
                relative: path.into(),
            }),
        };
        assert_eq!(exact_pid(&[p(2, COMM)]).unwrap(), Some(2));
        assert_eq!(
            exact_pid(&[p(7, &format!("file://{COMM}"))]).unwrap(),
            Some(7)
        );
        assert_eq!(exact_pid(&[p(8, &format!("{COMM}Helper"))]).unwrap(), None);
        assert!(exact_pid(&[p(1, COMM)]).is_err());
        assert!(exact_pid(&[p(3, COMM), p(4, COMM)]).is_err());
    }
    #[test]
    fn bundled_payloads_valid() {
        payloads(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../App/CarrierPayloads")
                .as_path(),
        )
        .unwrap();
    }
}

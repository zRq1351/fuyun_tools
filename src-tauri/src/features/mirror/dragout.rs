//! 手机文件拖拽导出到电脑：OS 级拖放（DoDragDrop + 虚拟文件）。
//!
//! 方案：拖放目标（资源管理器/桌面）请求 `FileGroupDescriptorW` 拿文件元信息，
//! 请求 `FileContents` 时才真正执行 `adb pull` 到临时目录并返回内容（延迟拉取），
//! 因此拖拽启动零等待，掉落到目标时才传输。
//!
//! 用法：前端在导出面板按下文件并拖出（>6px）时调用 `mirror_drag_export`。

use super::adb;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{implement, AsImpl, Error as WError, Result as WResult, HRESULT};
use windows::Win32::Foundation::{
    GlobalFree, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC, DV_E_LINDEX, E_FAIL, E_NOTIMPL,
    HGLOBAL, OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::System::Com::{
    IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, OleUninitialize, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, CFSTR_PREFERREDDROPEFFECT, FD_FILESIZE,
    FILEDESCRIPTORW,
};

/// 一个待拖出的手机文件。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DragItem {
    pub name: String,
    pub size: u64,
    pub path: String,
    /// 截图模式：内容直接由前端提供（当前画面 PNG），无需 adb pull。
    #[serde(default)]
    pub local_png: Option<Vec<u8>>,
}

// ---------- 画布智能拖出（识别结果晚绑定） ----------

/// 识别完成后的拖出内容。
enum Resolved {
    /// 按下点识别到真实文件
    Remote {
        name: String,
        path: String,
        size: u64,
    },
    /// 兜底：拖出当前画面截图
    Screenshot { name: String, bytes: Vec<u8> },
}

/// 画布拖出的晚绑定状态：DoDragDrop 立即启动，识别并行进行；
/// 拖放目标请求文件元数据（FILEDESCRIPTOR）时才等待识别结果（有上限）。
struct SmartDrag {
    fallback: Resolved,
    rx: Mutex<Option<std::sync::mpsc::Receiver<Resolved>>>,
    resolved: Mutex<Option<std::sync::Arc<Resolved>>>,
}

impl SmartDrag {
    /// 获取最终内容（首次调用会等待识别结果，最多 8 秒）。
    fn resolve(&self) -> std::sync::Arc<Resolved> {
        let mut cached = self.resolved.lock().unwrap();
        if let Some(v) = cached.as_ref() {
            return v.clone();
        }
        let recv = self.rx.lock().unwrap().take();
        let resolved = match recv {
            Some(rx) => rx.recv_timeout(std::time::Duration::from_secs(8)).ok(),
            None => None,
        };
        let v = std::sync::Arc::new(resolved.unwrap_or_else(|| match &self.fallback {
            Resolved::Screenshot { name, bytes } => Resolved::Screenshot {
                name: name.clone(),
                bytes: bytes.clone(),
            },
            _ => unreachable!(),
        }));
        log::info!(
            "[mirror-drag] 拖出内容解析: {}",
            match &*v {
                Resolved::Remote { name, .. } => format!("真实文件 {}", name),
                Resolved::Screenshot { name, .. } => format!("截图 {}", name),
            }
        );
        *cached = Some(v.clone());
        v
    }
}

/// 按下点识别真实文件：dump UI 找含扩展名的节点文本 → 常用目录按名搜索。
/// 返回 (文件名, 远端路径, 大小)。
fn recognize_file_at(adb: &Path, serial: &str, x: i32, y: i32) -> Option<(String, String, u64)> {
    use std::time::Duration;

    let dump_path = "/sdcard/fuyun_drag_dump.xml";
    if adb::run_adb(
        adb,
        &["-s", serial, "shell", "uiautomator", "dump", dump_path],
        Duration::from_secs(8),
    )
    .is_err()
    {
        return None;
    }
    let xml = adb::run_adb(
        adb,
        &["-s", serial, "shell", "cat", dump_path],
        Duration::from_secs(8),
    )
    .ok()?;
    let _ = adb::run_adb(
        adb,
        &["-s", serial, "shell", "rm", "-f", dump_path],
        Duration::from_secs(5),
    );

    let node_re = regex::Regex::new(r"<node[^>]*>").ok()?;
    let bounds_re = regex::Regex::new(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]").ok()?;
    let attr = |tag: &str, name: &str| -> Option<String> {
        regex::Regex::new(&format!(r#"{}="([^"]*)""#, name))
            .ok()?
            .captures(tag)?
            .get(1)
            .map(|m| m.as_str().to_string())
    };

    // 已知文件扩展名（可扩展）
    const EXTS: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "mp4", "mov", "avi", "mkv", "mp3",
        "wav", "flac", "aac", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv",
        "zip", "rar", "7z", "apk", "epub",
    ];
    let is_file_name = |t: &str| -> bool {
        let t = t.trim();
        if t.is_empty() || t.contains('\n') || t.len() > 128 {
            return false;
        }
        let lower = t.to_lowercase();
        EXTS.iter().any(|e| lower.ends_with(&format!(".{}", e)))
    };

    // 找包含按下点且文本像文件名的节点
    let mut name: Option<String> = None;
    for tag in node_re.find_iter(&xml) {
        let tag = tag.as_str();
        let text = attr(tag, "text").unwrap_or_default();
        let desc = attr(tag, "content-desc").unwrap_or_default();
        let candidate = if is_file_name(&text) {
            text
        } else if is_file_name(&desc) {
            desc
        } else {
            continue;
        };
        if let Some(b) = bounds_re.captures(tag) {
            let x1: i32 = b[1].parse().ok()?;
            let y1: i32 = b[2].parse().ok()?;
            let x2: i32 = b[3].parse().ok()?;
            let y2: i32 = b[4].parse().ok()?;
            if x >= x1 && x < x2 && y >= y1 && y < y2 {
                name = Some(candidate.trim().to_string());
                break;
            }
        }
    }
    let name = name?;
    log::info!("[mirror-drag] 识别到文件名: {}", name);

    // 常用目录按名搜索（找不到则失败）
    let dirs = [
        "/sdcard/Download",
        "/sdcard/DCIM",
        "/sdcard/Pictures",
        "/sdcard/Documents",
        "/sdcard/Movies",
        "/sdcard/Music",
        "/sdcard/Android/data",
    ];
    let escaped = name.replace('\'', "");
    let cmd = format!(
        "find {} -name '{}' 2>/dev/null | head -1",
        dirs.join(" "),
        escaped
    );
    let found = adb::run_adb(adb, &["-s", serial, "shell", &cmd], Duration::from_secs(15)).ok()?;
    let path = found.lines().next()?.trim().to_string();
    if path.is_empty() || !path.starts_with('/') {
        return None;
    }
    // 取文件大小（失败不阻塞，size=0 也可拖）
    let size = adb::run_adb(
        adb,
        &[
            "-s",
            serial,
            "shell",
            &format!("ls -l '{}' 2>/dev/null", path),
        ],
        Duration::from_secs(8),
    )
    .ok()
    .and_then(|out| {
        out.split_whitespace()
            .nth(4)
            .and_then(|s| s.parse::<u64>().ok())
    })
    .unwrap_or(0);
    Some((name, path, size))
}

/// 画布智能拖出：立即启动 DoDragDrop；并行识别按下点的真实文件，
/// 识别命中则拖真实文件，否则拖当前画面截图。
pub fn begin_smart_drag(
    adb: PathBuf,
    serial: String,
    screenshot: Vec<u8>,
    x: i32,
    y: i32,
) -> Result<(), String> {
    if screenshot.is_empty() {
        return Err("截图数据为空".into());
    }
    std::thread::spawn(move || {
        unsafe {
            if let Err(e) = OleInitialize(None) {
                log::warn!("[mirror-drag] OleInitialize 失败: {}", e);
                return;
            }
            let formats = register_formats();
            let fallback = Resolved::Screenshot {
                name: format!(
                    "手机截屏_{}.png",
                    chrono::Local::now().format("%Y%m%d_%H%M%S")
                ),
                bytes: screenshot,
            };
            let (tx, rx) = std::sync::mpsc::channel::<Resolved>();
            {
                let adb2 = adb.clone();
                let serial2 = serial.clone();
                std::thread::spawn(move || {
                    if let Some((name, path, size)) = recognize_file_at(&adb2, &serial2, x, y) {
                        let _ = tx.send(Resolved::Remote { name, path, size });
                    }
                    // 未识别到：不发送（接收端等待超时后用截图兜底）
                });
            }
            let data: IDataObject = DragDataObject {
                formats,
                items: Vec::new(),
                adb,
                serial,
                tmp_files: Mutex::new(Vec::new()),
                smart: Some(SmartDrag {
                    fallback,
                    rx: Mutex::new(Some(rx)),
                    resolved: Mutex::new(None),
                }),
            }
            .into();
            let source: IDropSource = DragSource.into();
            let mut effect = DROPEFFECT_NONE;
            let hr = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
            OleUninitialize();
            cleanup_after_drag(&data);
            log::info!(
                "[mirror-drag] 智能拖放结束: hr={:#x} effect={}",
                hr.0,
                effect.0
            );
        }
    });
    Ok(())
}

/// 注册的剪贴板格式（进程内只注册一次即可，但注册很便宜，每次拖放注册一遍也安全）。
struct Formats {
    file_desc: u16,
    file_contents: u16,
    pref_drop: u16,
}

fn register_formats() -> Formats {
    unsafe {
        Formats {
            file_desc: RegisterClipboardFormatW(CFSTR_FILEDESCRIPTORW) as u16,
            file_contents: RegisterClipboardFormatW(CFSTR_FILECONTENTS) as u16,
            pref_drop: RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) as u16,
        }
    }
}

// ---------- IDataObject ----------

#[implement(IDataObject)]
struct DragDataObject {
    formats: Formats,
    /// 面板拖拽模式：预定的文件列表（smart 模式下为空）。
    items: Vec<DragItem>,
    /// 画布智能拖拽模式：识别结果晚绑定。
    smart: Option<SmartDrag>,
    adb: PathBuf,
    serial: String,
    /// 本次拖放产生的临时文件（拖放结束后统一清理）。
    tmp_files: Mutex<Vec<PathBuf>>,
}

impl DragDataObject {
    fn tmp_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "fuyun_mirror_drag_{}_{}",
            std::process::id(),
            stamp
        ))
    }

    fn sanitize_name(name: &str) -> String {
        name.chars()
            .map(|c| match c {
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }

    /// 拉取远端文件到临时目录，返回全局内存中的内容。
    fn pull_remote(&self, idx: usize, path: &str, name: &str) -> WResult<HGLOBAL> {
        let dir = Self::tmp_dir();
        let _ = std::fs::create_dir_all(&dir);
        let local = dir.join(format!("{}_{}", idx, Self::sanitize_name(name)));
        let local_str = local.to_string_lossy().to_string();

        if !local.exists() {
            super::adb::run_adb(
                &self.adb,
                &["-s", &self.serial, "pull", path, &local_str],
                std::time::Duration::from_secs(300),
            )
            .map_err(|e| {
                log::warn!("[mirror-drag] 拉取失败 {}: {}", path, e);
                WError::from_hresult(E_FAIL)
            })?;
            if let Ok(mut list) = self.tmp_files.lock() {
                list.push(local.clone());
            }
            log::info!("[mirror-drag] 已拉取 {} -> {}", path, local_str);
        }

        let bytes = std::fs::read(&local).map_err(|e| {
            log::warn!("[mirror-drag] 读取临时文件失败 {}: {}", local_str, e);
            WError::from_hresult(E_FAIL)
        })?;
        Ok(alloc_bytes(&bytes))
    }

    /// 拉取第 idx 个文件到临时目录，返回全局内存中的内容。
    fn pull_item(&self, idx: usize) -> WResult<HGLOBAL> {
        let item = self
            .items
            .get(idx)
            .ok_or_else(|| WError::from_hresult(DV_E_LINDEX))?;
        // 截图模式：内容已在前端捕获
        if let Some(bytes) = &item.local_png {
            return Ok(alloc_bytes(bytes));
        }
        self.pull_remote(idx, &item.path, &item.name)
    }

    /// 构造 FILEGROUPDESCRIPTORW（cItems + FILEDESCRIPTORW 数组）。
    fn build_file_descriptors(&self) -> WResult<HGLOBAL> {
        if let Some(smart) = &self.smart {
            return self.build_descriptors_for(&smart_descriptor(smart));
        }
        self.build_descriptors_for(
            &self
                .items
                .iter()
                .map(|i| DescriptorSource::Item(i.clone()))
                .collect::<Vec<_>>(),
        )
    }

    fn build_descriptors_for(&self, items: &[DescriptorSource]) -> WResult<HGLOBAL> {
        let item_size = std::mem::size_of::<FILEDESCRIPTORW>();
        let total = 4 + items.len() * item_size;
        let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, total) }?;
        let p = unsafe { GlobalLock(h) };
        if p.is_null() {
            unsafe {
                let _ = GlobalFree(Some(h));
            }
            return Err(WError::from_hresult(E_FAIL));
        }
        unsafe {
            let base = p as *mut u8;
            // cItems (u32, packed 布局下偏移 0)
            (base as *mut u32).write(items.len() as u32);
            for (i, item) in items.iter().enumerate() {
                let (name, size) = item.name_size();
                let mut d = FILEDESCRIPTORW {
                    dwFlags: FD_FILESIZE.0 as u32,
                    nFileSizeHigh: (size >> 32) as u32,
                    nFileSizeLow: (size & 0xFFFF_FFFF) as u32,
                    ..Default::default()
                };
                for (j, ch) in Self::sanitize_name(&name)
                    .encode_utf16()
                    .take(259)
                    .enumerate()
                {
                    d.cFileName[j] = ch;
                }
                // FILEGROUPDESCRIPTORW 是 packed(1)：fgd 紧跟 cItems 之后
                let slot = base.add(4 + i * item_size) as *mut FILEDESCRIPTORW;
                core::ptr::write_unaligned(slot, d);
            }
            let _ = GlobalUnlock(h);
        }
        Ok(h)
    }

    fn is_supported_format(&self, fe: &FORMATETC) -> bool {
        fe.dwAspect == DVASPECT_CONTENT.0
            && (fe.tymed & TYMED_HGLOBAL.0 as u32) != 0
            && (fe.cfFormat == self.formats.file_desc
                || fe.cfFormat == self.formats.file_contents
                || fe.cfFormat == self.formats.pref_drop)
    }

    /// 智能模式：按解析结果返回内容。
    fn smart_content(&self, smart: &SmartDrag) -> WResult<HGLOBAL> {
        let resolved = smart.resolve();
        match &*resolved {
            Resolved::Screenshot { bytes, .. } => Ok(alloc_bytes(bytes)),
            Resolved::Remote { path, name, .. } => self.pull_remote(0, path, name),
        }
    }
}

/// 描述符来源：面板条目或智能解析结果。
enum DescriptorSource {
    Item(DragItem),
    Smart(std::sync::Arc<Resolved>),
}

impl DescriptorSource {
    fn name_size(&self) -> (String, u64) {
        match self {
            DescriptorSource::Item(i) => (i.name.clone(), i.size),
            DescriptorSource::Smart(r) => match &**r {
                Resolved::Remote { name, size, .. } => (name.clone(), *size),
                Resolved::Screenshot { name, bytes } => (name.clone(), bytes.len() as u64),
            },
        }
    }
}

fn smart_descriptor(smart: &SmartDrag) -> Vec<DescriptorSource> {
    vec![DescriptorSource::Smart(smart.resolve())]
}

fn alloc_bytes(bytes: &[u8]) -> HGLOBAL {
    let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()).expect("GlobalAlloc") };
    let p = unsafe { GlobalLock(h) };
    if !p.is_null() {
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len()) };
        unsafe {
            let _ = GlobalUnlock(h);
        }
    }
    h
}

impl IDataObject_Impl for DragDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> WResult<STGMEDIUM> {
        if pformatetcin.is_null() {
            return Err(WError::from_hresult(DV_E_FORMATETC));
        }
        let fe = unsafe { &*pformatetcin };
        if !self.is_supported_format(fe) {
            return Err(WError::from_hresult(DV_E_FORMATETC));
        }
        let h = if let Some(smart) = &self.smart {
            // 智能模式：单文件，按解析结果返回
            if fe.cfFormat == self.formats.file_desc {
                self.build_file_descriptors()?
            } else if fe.cfFormat == self.formats.file_contents {
                self.smart_content(smart)?
            } else {
                alloc_bytes(&DROPEFFECT_COPY.0.to_ne_bytes())
            }
        } else if fe.cfFormat == self.formats.file_desc {
            self.build_file_descriptors()?
        } else if fe.cfFormat == self.formats.file_contents {
            let idx = if fe.lindex < 0 {
                if self.items.len() == 1 {
                    0
                } else {
                    return Err(WError::from_hresult(DV_E_LINDEX));
                }
            } else {
                fe.lindex as usize
            };
            self.pull_item(idx)?
        } else {
            alloc_bytes(&DROPEFFECT_COPY.0.to_ne_bytes())
        };
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: h },
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if pformatetc.is_null() {
            return DV_E_FORMATETC;
        }
        let fe = unsafe { &*pformatetc };
        if self.is_supported_format(fe) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> WResult<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(WError::from_hresult(E_NOTIMPL));
        }
        let formats = vec![
            FORMATETC {
                cfFormat: self.formats.file_desc,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
            FORMATETC {
                cfFormat: self.formats.file_contents,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
            FORMATETC {
                cfFormat: self.formats.pref_drop,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            },
        ];
        Ok(FormatEnumerator {
            formats,
            pos: Cell::new(0),
        }
        .into())
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> WResult<()> {
        Err(WError::from_hresult(E_NOTIMPL))
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        _pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: windows::core::BOOL,
    ) -> WResult<()> {
        Err(WError::from_hresult(E_NOTIMPL))
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: windows::core::Ref<windows::Win32::System::Com::IAdviseSink>,
    ) -> WResult<u32> {
        Err(WError::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _dwconnection: u32) -> WResult<()> {
        Err(WError::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }

    fn EnumDAdvise(&self) -> WResult<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(WError::from_hresult(OLE_E_ADVISENOTSUPPORTED))
    }
}

// ---------- IEnumFORMATETC ----------

#[implement(IEnumFORMATETC)]
struct FormatEnumerator {
    formats: Vec<FORMATETC>,
    pos: Cell<usize>,
}

impl IEnumFORMATETC_Impl for FormatEnumerator_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        if rgelt.is_null() {
            return E_FAIL;
        }
        unsafe {
            let want = celt as usize;
            let total = self.formats.len();
            let start = self.pos.get().min(total);
            let end = (start + want).min(total);
            let n = end - start;
            for (i, f) in self.formats[start..end].iter().enumerate() {
                rgelt.add(i).write(*f);
            }
            self.pos.set(end);
            if !pceltfetched.is_null() {
                pceltfetched.write(n as u32);
            }
            if n == want && want > 0 {
                S_OK
            } else {
                windows::Win32::Foundation::S_FALSE
            }
        }
    }

    fn Skip(&self, celt: u32) -> WResult<()> {
        let total = self.formats.len();
        let start = self.pos.get().min(total);
        self.pos.set((start + celt as usize).min(total));
        Ok(())
    }

    fn Reset(&self) -> WResult<()> {
        self.pos.set(0);
        Ok(())
    }

    fn Clone(&self) -> WResult<IEnumFORMATETC> {
        Ok(FormatEnumerator {
            formats: self.formats.clone(),
            pos: Cell::new(self.pos.get()),
        }
        .into())
    }
}

// ---------- IDropSource ----------

#[implement(IDropSource)]
struct DragSource;

impl IDropSource_Impl for DragSource_Impl {
    fn QueryContinueDrag(
        &self,
        fescapepressed: windows::core::BOOL,
        grfkeystate: MODIFIERKEYS_FLAGS,
    ) -> HRESULT {
        if fescapepressed.as_bool() {
            windows::Win32::Foundation::DRAGDROP_S_CANCEL
        } else if (grfkeystate.0 & MK_LBUTTON.0) == 0 {
            windows::Win32::Foundation::DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

// ---------- 入口 ----------

/// 拖放结束后清理临时文件。
fn cleanup_after_drag(data: &IDataObject) {
    let tmps: Vec<PathBuf> = unsafe {
        let inner: &DragDataObject = data.as_impl();
        inner
            .tmp_files
            .lock()
            .map(|mut l| std::mem::take(&mut *l))
            .unwrap_or_default()
    };
    for t in &tmps {
        let _ = std::fs::remove_file(t);
    }
    if let Some(dir) = tmps.first().and_then(|f| f.parent().map(Path::to_path_buf)) {
        let _ = std::fs::remove_dir(dir);
    }
    log::info!("[mirror-drag] 清理临时文件 {} 个", tmps.len());
}

/// 启动 OS 级拖放（后台线程，直到用户释放鼠标/ESC）。命令立即返回。
pub fn begin_drag(adb: PathBuf, serial: String, items: Vec<DragItem>) -> Result<(), String> {
    if items.is_empty() {
        return Err("未选择任何文件".into());
    }
    std::thread::spawn(move || unsafe {
        // DoDragDrop 需要 OLE STA；本线程是专用线程，初始化/反初始化配对即可
        if let Err(e) = OleInitialize(None) {
            log::warn!("[mirror-drag] OleInitialize 失败: {}", e);
            return;
        }
        let formats = register_formats();
        let data: IDataObject = DragDataObject {
            formats,
            items,
            smart: None,
            adb,
            serial,
            tmp_files: Mutex::new(Vec::new()),
        }
        .into();
        let source: IDropSource = DragSource.into();
        let mut effect = DROPEFFECT_NONE;
        let hr = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
        OleUninitialize();
        cleanup_after_drag(&data);
        log::info!("[mirror-drag] 拖放结束: hr={:#x} effect={}", hr.0, effect.0);
    });
    Ok(())
}

//! 手机文件拖拽导出到电脑：OS 级拖放（DoDragDrop + 虚拟文件）。
//!
//! 方案：拖放目标（资源管理器/桌面）请求 `FileGroupDescriptorW` 拿文件元信息，
//! 请求 `FileContents` 时才真正执行 `adb pull` 到临时目录并返回内容（延迟拉取），
//! 因此拖拽启动零等待，掉落到目标时才传输。
//!
//! 用法：前端在导出面板按下文件并拖出（>6px）时调用 `mirror_drag_export`。

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
    items: Vec<DragItem>,
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

    /// 拉取第 idx 个文件到临时目录，返回全局内存中的内容。
    fn pull_item(&self, idx: usize) -> WResult<HGLOBAL> {
        let item = self
            .items
            .get(idx)
            .ok_or_else(|| WError::from_hresult(DV_E_LINDEX))?;
        let dir = Self::tmp_dir();
        let _ = std::fs::create_dir_all(&dir);
        let local = dir.join(format!("{}_{}", idx, Self::sanitize_name(&item.name)));
        let local_str = local.to_string_lossy().to_string();

        if !local.exists() {
            super::adb::run_adb(
                &self.adb,
                &["-s", &self.serial, "pull", &item.path, &local_str],
                std::time::Duration::from_secs(300),
            )
            .map_err(|e| {
                log::warn!("[mirror-drag] 拉取失败 {}: {}", item.path, e);
                WError::from_hresult(E_FAIL)
            })?;
            if let Ok(mut list) = self.tmp_files.lock() {
                list.push(local.clone());
            }
            log::info!("[mirror-drag] 已拉取 {} -> {}", item.path, local_str);
        }

        let bytes = std::fs::read(&local).map_err(|e| {
            log::warn!("[mirror-drag] 读取临时文件失败 {}: {}", local_str, e);
            WError::from_hresult(E_FAIL)
        })?;
        Ok(alloc_bytes(&bytes))
    }

    /// 构造 FILEGROUPDESCRIPTORW（cItems + FILEDESCRIPTORW 数组）。
    fn build_file_descriptors(&self) -> WResult<HGLOBAL> {
        let item_size = std::mem::size_of::<FILEDESCRIPTORW>();
        let total = 4 + self.items.len() * item_size;
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
            (base as *mut u32).write(self.items.len() as u32);
            for (i, item) in self.items.iter().enumerate() {
                let mut d = FILEDESCRIPTORW {
                    dwFlags: FD_FILESIZE.0 as u32,
                    nFileSizeHigh: (item.size >> 32) as u32,
                    nFileSizeLow: (item.size & 0xFFFF_FFFF) as u32,
                    ..Default::default()
                };
                for (j, ch) in Self::sanitize_name(&item.name)
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
        let h = if fe.cfFormat == self.formats.file_desc {
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
            adb,
            serial,
            tmp_files: Mutex::new(Vec::new()),
        }
        .into();
        let source: IDropSource = DragSource.into();
        let mut effect = DROPEFFECT_NONE;
        let hr = DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect);
        OleUninitialize();

        // 清理本次拖放产生的临时文件
        let tmps: Vec<PathBuf> = {
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

        log::info!(
            "[mirror-drag] 拖放结束: hr={:#x} effect={} 清理临时文件 {} 个",
            hr.0,
            effect.0,
            tmps.len()
        );
    });
    Ok(())
}

//! The file's Windows Shell menu, with the handlers registered for its type.

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_EXPLORE, CMF_EXTENDEDVERBS, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    IContextMenu, IContextMenu2, IContextMenu3, IShellFolder, SHBindToParent, SHParseDisplayName,
};
use windows::Win32::UI::WindowsAndMessaging::HMENU;
use windows::core::{Interface, PCSTR, PCWSTR};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, GetCursorPos, PostMessageW,
    SW_SHOWNORMAL, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

/// Show the file's classic Explorer menu in a Shell apartment beside the app's window.
pub fn show_file_menu(
    title: &str,
    path: PathBuf,
    moved_away: bool,
    at: Option<egui::Pos2>,
    ctx: egui::Context,
) -> Result<(), String> {
    let owner = super::find_own_window(title).ok_or("window not found")?;
    let mut point = windows_sys::Win32::Foundation::POINT::default();
    // SAFETY: the coordinates are copied before starting the menu's worker.
    let extended = unsafe {
        if let Some(at) = at {
            let scale = ctx.pixels_per_point();
            point.x = (at.x * scale).round() as i32;
            point.y = (at.y * scale).round() as i32;
            windows_sys::Win32::Graphics::Gdi::ClientToScreen(owner, &mut point);
        } else {
            GetCursorPos(&mut point);
        }
        GetKeyState(VK_SHIFT as i32) < 0
    };
    let owner = owner.expose_provenance();
    super::with_shared_input("file context menu", move |_| {
        let owner = HWND(std::ptr::with_exposed_provenance_mut(owner));
        let point = POINT {
            x: point.x,
            y: point.y,
        };
        crate::dialogs::file_menu_now(&path, moved_away, |path| {
            Popup::show(owner, point, path, extended).map_err(|e| e.to_string())
        });
        ctx.request_repaint();
        || {}
    })
}

struct ItemId(*mut ITEMIDLIST);

impl Drop for ItemId {
    fn drop(&mut self) {
        // SAFETY: the Shell allocated this item ID for SHParseDisplayName.
        unsafe { CoTaskMemFree(Some(self.0.cast())) }
    }
}

struct Popup {
    window: HWND,
    menu: HMENU,
}

impl Drop for Popup {
    fn drop(&mut self) {
        // SAFETY: both handles were created on this thread for this menu.
        unsafe {
            DestroyMenu(self.menu.0);
            DestroyWindow(self.window.0);
        }
    }
}

impl Popup {
    fn show(owner: HWND, point: POINT, path: &Path, extended: bool) -> windows::core::Result<()> {
        const FIRST: u32 = 1;
        const LAST: u32 = 0x7fff;

        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut item = ItemId(null_mut());
        // SAFETY: this worker has a Shell apartment. The path, PIDLs and COM objects
        // outlive all the calls that borrow them.
        let context = unsafe {
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut item.0, 0, None)?;
            let mut child = null_mut();
            let folder = SHBindToParent::<IShellFolder>(item.0, Some(&mut child))?;
            folder.GetUIObjectOf::<IContextMenu>(owner, &[child.cast_const()], None)?
        };
        // The hidden popup window belongs to this thread and receives menu messages, while
        // commands such as Properties and Delete use the app window as their owner.
        let class = windows::core::w!("STATIC");
        // SAFETY: The standard window class needs no custom creation data.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                null(),
                WS_POPUP,
                point.x,
                point.y,
                0,
                0,
                owner.0,
                null_mut(),
                null_mut(),
                null(),
            )
        };
        if window.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        let popup = Popup {
            window: HWND(window),
            // SAFETY: creates a menu owned by this worker.
            menu: HMENU(unsafe { CreatePopupMenu() }),
        };
        if popup.menu.0.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        let mut flags = CMF_EXPLORE;
        if extended {
            flags |= CMF_EXTENDEDVERBS;
        }
        let messages = MenuMessages {
            with_result: context.cast::<IContextMenu3>().ok(),
            without_result: context.cast::<IContextMenu2>().ok(),
        };
        // SAFETY: the subclass borrows messages only in the menu loop on this thread.
        // Always remove it before leaving this scope.
        let command = unsafe {
            context
                .QueryContextMenu(popup.menu, 0, FIRST, LAST, flags)
                .ok()?;
            let data = std::ptr::from_ref(&messages).expose_provenance();
            if SetWindowSubclass(window, Some(menu_messages), 1, data) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            SetForegroundWindow(owner.0);
            let command = TrackPopupMenuEx(
                popup.menu.0,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                window,
                null(),
            ) as u32;
            RemoveWindowSubclass(window, Some(menu_messages), 1);
            PostMessageW(owner.0, WM_NULL, 0, 0);
            command
        };
        if (FIRST..=LAST).contains(&command) {
            let invoke = CMINVOKECOMMANDINFOEX {
                cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                fMask: CMIC_MASK_PTINVOKE,
                hwnd: owner,
                // The Shell interprets low-valued pointers as command offsets.
                lpVerb: PCSTR(std::ptr::without_provenance((command - FIRST) as usize)),
                nShow: SW_SHOWNORMAL,
                ptInvoke: point,
                ..Default::default()
            };
            // SAFETY: the extended structure has the CommandInfo prefix and reports its size.
            unsafe {
                context.InvokeCommand(std::ptr::from_ref(&invoke).cast::<CMINVOKECOMMANDINFO>())?
            };
        }
        Ok(())
    }
}

struct MenuMessages {
    with_result: Option<IContextMenu3>,
    without_result: Option<IContextMenu2>,
}

unsafe extern "system" fn menu_messages(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    if matches!(
        msg,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        // SAFETY: the subclass is installed only while the worker's MenuMessages is alive.
        let handler = unsafe { &*std::ptr::with_exposed_provenance::<MenuMessages>(data) };
        let mut result = windows::Win32::Foundation::LRESULT(0);
        let w = windows::Win32::Foundation::WPARAM(wparam);
        let l = windows::Win32::Foundation::LPARAM(lparam);
        if let Some(menu) = &handler.with_result {
            if unsafe { menu.HandleMenuMsg2(msg, w, l, Some(&mut result)) }.is_ok() {
                return result.0;
            }
        } else if msg != WM_MENUCHAR
            && let Some(menu) = &handler.without_result
            && unsafe { menu.HandleMenuMsg(msg, w, l) }.is_ok()
        {
            return 0;
        }
    }
    // SAFETY: forward any message the Shell didn't handle to the original window procedure.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

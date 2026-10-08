#![cfg(windows)]

use std::{
    ffi::c_void,
    fs,
    path::PathBuf,
    process::Command as ProcessCommand,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use mue_core::{
    ConversionChoice, Request, data_dir,
    profiles::{MediaKind, OutputFormat, Settings},
};
use windows::{
    Win32::{
        Foundation::{
            CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_NOTIMPL,
            E_POINTER, S_FALSE, S_OK,
        },
        System::{
            Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl},
            LibraryLoader::{
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
                GetModuleHandleExW,
            },
        },
        UI::Shell::{
            ECF_HASSUBCOMMANDS, ECS_DISABLED, ECS_ENABLED, ECS_HIDDEN, IEnumExplorerCommand,
            IEnumExplorerCommand_Impl, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray,
            SHStrDupW, SIGDN_FILESYSPATH,
        },
    },
    core::{
        BOOL, Error, GUID, HRESULT, HSTRING, IUnknown, Interface, PWSTR, Ref, Result, implement,
    },
};

const CLASS_ID: GUID = GUID::from_u128(0xb28af997_f05d_4217_8ce9_1547ea69b6a8);
const PROFILES_CLASS_ID: GUID = GUID::from_u128(0xb28af997_f05d_4217_8ce9_1547ea69b6a9);
static LIVE_OBJECTS: AtomicUsize = AtomicUsize::new(0);
static SERVER_LOCKS: AtomicUsize = AtomicUsize::new(0);

struct Lifetime;
impl Lifetime {
    fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::Relaxed);
        Self
    }
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
enum Node {
    Profiles,
    Convert {
        choice: ConversionChoice,
        title: String,
        kind: MediaKind,
        canonical: GUID,
    },
}

#[implement(IExplorerCommand)]
struct ExplorerCommand {
    node: Node,
    selection: Arc<Mutex<Option<MediaKind>>>,
    _lifetime: Lifetime,
}

impl ExplorerCommand {
    fn create(node: Node, selection: Arc<Mutex<Option<MediaKind>>>) -> IExplorerCommand {
        Self {
            node,
            selection,
            _lifetime: Lifetime::new(),
        }
        .into()
    }
}

impl IExplorerCommand_Impl for ExplorerCommand_Impl {
    fn GetTitle(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        let title = match &self.node {
            Node::Profiles => "Profiles",
            Node::Convert { title, .. } => title,
        };
        unsafe { SHStrDupW(&HSTRING::from(title)) }
    }

    fn GetIcon(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        let icon = module_directory()?.join("Assets").join("Mue.ico");
        unsafe { SHStrDupW(&HSTRING::from(icon.as_os_str())) }
    }

    fn GetToolTip(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        unsafe {
            SHStrDupW(&HSTRING::from(
                "Convert with Mue. Your original file is preserved.",
            ))
        }
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(match &self.node {
            Node::Profiles => PROFILES_CLASS_ID,
            Node::Convert { canonical, .. } => *canonical,
        })
    }

    fn GetState(&self, items: Ref<IShellItemArray>, _: BOOL) -> Result<u32> {
        let Some(items) = items.as_ref() else {
            return Ok(ECS_HIDDEN.0 as u32);
        };
        let kind = selection_kind(items).ok().flatten();
        *self.selection.lock().unwrap() = kind;
        let Some(kind) = kind else {
            return Ok(ECS_HIDDEN.0 as u32);
        };
        Ok(match &self.node {
            Node::Convert { kind: expected, .. } if kind != *expected => ECS_HIDDEN.0,
            Node::Profiles
                if Settings::load().map_or(true, |settings| {
                    !settings.profiles.iter().any(|p| p.format.kind() == kind)
                }) =>
            {
                ECS_DISABLED.0
            }
            _ => ECS_ENABLED.0,
        } as u32)
    }

    fn Invoke(&self, items: Ref<IShellItemArray>, _: Ref<IBindCtx>) -> Result<()> {
        let Node::Convert { choice, kind, .. } = &self.node else {
            return Err(E_NOTIMPL.into());
        };
        let items = items.as_ref().ok_or_else(|| Error::from(E_INVALIDARG))?;
        if selection_kind(items)? != Some(*kind) {
            return Err(E_INVALIDARG.into());
        }
        let files = selected_paths(items)?;
        let request = Request::Convert {
            choice: choice.clone(),
            files,
        };
        let directory = data_dir().map_err(com_error)?.join("requests");
        fs::create_dir_all(&directory).map_err(com_error)?;
        let path = directory.join(format!("{}.json", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec(&request).map_err(com_error)?;
        fs::write(&path, bytes).map_err(com_error)?;
        let result = ProcessCommand::new(module_directory()?.join("mue.exe"))
            .arg("--request-file")
            .arg(&path)
            .spawn();
        if let Err(error) = result {
            let _ = fs::remove_file(path);
            return Err(com_error(error));
        }
        Ok(())
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(if matches!(self.node, Node::Profiles) {
            ECF_HASSUBCOMMANDS.0 as u32
        } else {
            0
        })
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        let kind = *self.selection.lock().unwrap();
        let nodes: Vec<Node> = match self.node {
            Node::Profiles => Settings::load()
                .map_err(com_error)?
                .profiles
                .into_iter()
                .filter(|profile| kind.is_none_or(|kind| profile.format.kind() == kind))
                .map(|profile| Node::Convert {
                    canonical: GUID::from_u128(profile.id.as_u128()),
                    choice: ConversionChoice::Profile(profile.id),
                    title: profile.name,
                    kind: profile.format.kind(),
                })
                .collect(),
            _ => return Err(E_NOTIMPL.into()),
        };
        let commands = nodes
            .into_iter()
            .map(|node| ExplorerCommand::create(node, self.selection.clone()))
            .collect();
        Ok(CommandEnumerator {
            commands,
            position: Mutex::new(0),
            _lifetime: Lifetime::new(),
        }
        .into())
    }
}

fn selected_paths(items: &IShellItemArray) -> Result<Vec<PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    let count = unsafe { items.GetCount()? };
    if count == 0 || count > 1000 {
        return Err(E_INVALIDARG.into());
    }
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let item = unsafe { items.GetItemAt(index)? };
        let name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
        let path = PathBuf::from(std::ffi::OsString::from_wide(unsafe { name.as_wide() }));
        unsafe {
            CoTaskMemFree(Some(name.0 as _));
        }
        paths.push(path);
    }
    Ok(paths)
}

fn selection_kind(items: &IShellItemArray) -> Result<Option<MediaKind>> {
    let files = selected_paths(items)?;
    let Some(kind) = files.first().and_then(|file| MediaKind::for_path(file)) else {
        return Ok(None);
    };
    Ok(files
        .iter()
        .all(|file| MediaKind::for_path(file) == Some(kind))
        .then_some(kind))
}

fn module_directory() -> Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::HMODULE;
    let mut module = HMODULE::default();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(DllGetClassObject as *const () as *const u16),
            &mut module,
        )?;
    }
    let mut path = vec![0; 32768];
    let count = unsafe { GetModuleFileNameW(Some(module), &mut path) } as usize;
    if count == 0 || count == path.len() {
        return Err(E_FAIL.into());
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..count]));
    path.parent()
        .map(PathBuf::from)
        .ok_or_else(|| E_FAIL.into())
}

fn com_error(error: impl std::fmt::Display) -> Error {
    Error::new(E_FAIL, error.to_string())
}

#[implement(IEnumExplorerCommand)]
struct CommandEnumerator {
    commands: Vec<IExplorerCommand>,
    position: Mutex<usize>,
    _lifetime: Lifetime,
}

impl IEnumExplorerCommand_Impl for CommandEnumerator_Impl {
    fn Next(
        &self,
        count: u32,
        output: *mut Option<IExplorerCommand>,
        fetched: *mut u32,
    ) -> HRESULT {
        if output.is_null() || (fetched.is_null() && count != 1) {
            return E_POINTER;
        }
        let mut position = self.position.lock().unwrap();
        let available = self
            .commands
            .len()
            .saturating_sub(*position)
            .min(count as usize);
        unsafe {
            for index in 0..count as usize {
                output.add(index).write(None);
            }
            for index in 0..available {
                output
                    .add(index)
                    .write(Some(self.commands[*position + index].clone()));
            }
            if !fetched.is_null() {
                fetched.write(available as u32);
            }
        }
        *position += available;
        if available == count as usize {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, count: u32) -> Result<()> {
        let mut position = self.position.lock().unwrap();
        *position = position
            .saturating_add(count as usize)
            .min(self.commands.len());
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        *self.position.lock().unwrap() = 0;
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(CommandEnumerator {
            commands: self.commands.clone(),
            position: Mutex::new(*self.position.lock().unwrap()),
            _lifetime: Lifetime::new(),
        }
        .into())
    }
}

#[implement(IClassFactory)]
struct Factory {
    node: Node,
    _lifetime: Lifetime,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if object.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe {
            object.write(std::ptr::null_mut());
        }
        if outer.as_ref().is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let command = ExplorerCommand::create(self.node.clone(), Arc::new(Mutex::new(None)));
        unsafe { command.query(iid, object).ok() }
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            SERVER_LOCKS.fetch_add(1, Ordering::Relaxed);
        } else {
            // Keep compatibility with the declared Rust 1.90 minimum.
            #[allow(deprecated)]
            let _ = SERVER_LOCKS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_sub(1)
            });
        }
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    class: *const GUID,
    iid: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    if class.is_null() || iid.is_null() || object.is_null() {
        return E_POINTER;
    }
    unsafe {
        object.write(std::ptr::null_mut());
    }
    let requested = unsafe { *class };
    let node = if requested == PROFILES_CLASS_ID {
        Node::Profiles
    } else {
        let Some((index, format)) = OutputFormat::ALL
            .into_iter()
            .enumerate()
            .find(|(index, _)| {
                requested == GUID::from_u128(CLASS_ID.to_u128() + 10 + *index as u128)
            })
        else {
            return CLASS_E_CLASSNOTAVAILABLE;
        };
        Node::Convert {
            choice: ConversionChoice::Format(format),
            title: format!("Convert to {}", format.label()),
            kind: format.kind(),
            canonical: GUID::from_u128(CLASS_ID.to_u128() + 10 + index as u128),
        }
    };
    let factory: IClassFactory = Factory {
        node,
        _lifetime: Lifetime::new(),
    }
    .into();
    unsafe { factory.query(iid, object) }
}

#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE_OBJECTS.load(Ordering::Relaxed) == 0 && SERVER_LOCKS.load(Ordering::Relaxed) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

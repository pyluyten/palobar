use glib::translate::{from_glib_full, ToGlibPtr};
use gtk::{gdk, glib};
use libc::{c_int, c_void};

type LockHandle = *mut glib::gobject_ffi::GObject;
type LockRequest = unsafe extern "C" fn(LockHandle) -> c_int;
type AssignWindow =
    unsafe extern "C" fn(LockHandle, *mut gtk::ffi::GtkWindow, *mut gdk::ffi::GdkMonitor);

pub struct NativeLock {
    pub object: glib::Object,
    request: LockRequest,
    unlock: unsafe extern "C" fn(LockHandle),
    assign: AssignWindow,
}

impl NativeLock {
    pub fn new() -> Result<Self, &'static str> {
        // Resolve these from the layer-shell library already linked by the panel.
        // Older libraries can still run the other Palobar commands.
        macro_rules! session_symbol {
            ($name:literal, $type:ty) => {{
                let pointer = unsafe { libc::dlsym(libc::RTLD_DEFAULT, $name.as_ptr()) };
                if pointer.is_null() {
                    return Err("The lock screen requires GTK4 Layer Shell 1.2 or newer with session-lock support.");
                }
                unsafe { std::mem::transmute::<*mut c_void, $type>(pointer) }
            }};
        }
        let supported = session_symbol!(
            c"gtk_session_lock_is_supported",
            unsafe extern "C" fn() -> c_int
        );
        let new = session_symbol!(
            c"gtk_session_lock_instance_new",
            unsafe extern "C" fn() -> LockHandle
        );
        let request = session_symbol!(c"gtk_session_lock_instance_lock", LockRequest);
        let unlock = session_symbol!(
            c"gtk_session_lock_instance_unlock",
            unsafe extern "C" fn(LockHandle)
        );
        let assign = session_symbol!(
            c"gtk_session_lock_instance_assign_window_to_monitor",
            AssignWindow
        );
        if unsafe { supported() } == 0 {
            return Err("The compositor does not support Wayland session locking.");
        }
        let handle = unsafe { new() };
        if handle.is_null() {
            return Err("Could not create a Wayland session lock.");
        }
        let object: glib::Object = unsafe { from_glib_full(handle) };
        use glib::prelude::ObjectExt;
        if glib::subclass::SignalId::lookup("monitor", object.type_()).is_none() {
            return Err("The lock screen requires GTK4 Layer Shell 1.2 or newer.");
        }
        Ok(Self {
            object,
            request,
            unlock,
            assign,
        })
    }

    pub fn request(&self) -> bool {
        unsafe { (self.request)(self.object.to_glib_none().0) != 0 }
    }

    pub fn assign(&self, window: &gtk::Window, monitor: &gdk::Monitor) {
        unsafe {
            (self.assign)(
                self.object.to_glib_none().0,
                window.to_glib_none().0,
                monitor.to_glib_none().0,
            );
        }
    }

    pub fn unlock(&self) {
        unsafe { (self.unlock)(self.object.to_glib_none().0) }
    }
}

use libc::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;

const SERVICE: &CStr = c"astal-auth";
const SUCCESS: c_int = 0;
const BUFFER_ERROR: c_int = 5;
const CONVERSATION_ERROR: c_int = 19;
const DISALLOW_NULL_TOKEN: c_int = 1;
const PROMPT_HIDDEN: c_int = 1;
const PROMPT_VISIBLE: c_int = 2;
const ERROR_MESSAGE: c_int = 3;
const TEXT_INFO: c_int = 4;
const MAX_MESSAGES: c_int = 32;

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conversation {
    respond:
        unsafe extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

type Start = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    *const Conversation,
    *mut *mut c_void,
) -> c_int;
type PamCall = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

struct Library(*mut c_void);

impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.0);
        }
    }
}

struct Pam {
    _library: Library,
    start: Start,
    authenticate: PamCall,
    account: PamCall,
    end: PamCall,
}

impl Pam {
    fn open() -> Result<Self, &'static str> {
        // NixOS exposes the active system's PAM library outside the loader's default paths.
        let library = [
            c"libpam.so.0",
            c"/run/current-system/sw/lib/libpam.so.0",
        ]
        .into_iter()
        .find_map(|path| {
            let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            (!handle.is_null()).then(|| Library(handle))
        })
        .ok_or("Could not load libpam.so.0 from the system library paths or the active NixOS profile.")?;
        macro_rules! pam_symbol {
            ($name:literal, $type:ty) => {{
                let pointer = unsafe { libc::dlsym(library.0, $name.as_ptr()) };
                if pointer.is_null() {
                    return Err("The installed Linux-PAM library is missing a required function.");
                }
                unsafe { std::mem::transmute::<*mut c_void, $type>(pointer) }
            }};
        }
        let start = pam_symbol!(c"pam_start", Start);
        let authenticate = pam_symbol!(c"pam_authenticate", PamCall);
        let account = pam_symbol!(c"pam_acct_mgmt", PamCall);
        let end = pam_symbol!(c"pam_end", PamCall);
        Ok(Self {
            _library: library,
            start,
            authenticate,
            account,
            end,
        })
    }
}

pub struct Password(Vec<u8>);

impl Password {
    pub fn new(bytes: &CStr) -> Self {
        Self(bytes.to_bytes_with_nul().to_vec())
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            unsafe {
                ptr::write_volatile(byte, 0);
            }
        }
    }
}

struct Credentials<'a> {
    user: &'a CStr,
    password: &'a Password,
}

unsafe fn discard_responses(responses: *mut Response, len: usize) {
    for index in 0..len {
        let text = (*responses.add(index)).text;
        if !text.is_null() {
            for offset in 0..libc::strlen(text) {
                ptr::write_volatile(text.add(offset), 0);
            }
            libc::free(text.cast());
        }
    }
    libc::free(responses.cast());
}

unsafe extern "C" fn conversation(
    count: c_int,
    messages: *mut *const Message,
    response: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if !(1..=MAX_MESSAGES).contains(&count)
        || messages.is_null()
        || response.is_null()
        || data.is_null()
    {
        return CONVERSATION_ERROR;
    }
    *response = ptr::null_mut();
    let credentials = &*data.cast::<Credentials>();
    let len = count as usize;
    let responses = libc::calloc(len, std::mem::size_of::<Response>()).cast::<Response>();
    if responses.is_null() {
        return BUFFER_ERROR;
    }
    for index in 0..len {
        let Some(message) = (*messages.add(index)).as_ref() else {
            discard_responses(responses, len);
            return CONVERSATION_ERROR;
        };
        let value = match message.style {
            PROMPT_HIDDEN => credentials.password.0.as_ptr().cast::<c_char>(),
            PROMPT_VISIBLE => credentials.user.as_ptr(),
            ERROR_MESSAGE | TEXT_INFO => continue,
            _ => {
                discard_responses(responses, len);
                return CONVERSATION_ERROR;
            }
        };
        let text = libc::strdup(value);
        if text.is_null() {
            discard_responses(responses, len);
            return BUFFER_ERROR;
        }
        (*responses.add(index)).text = text;
    }
    // PAM owns the response array and its strings after a successful conversation.
    *response = responses;
    SUCCESS
}

pub fn session_user() -> Result<CString, &'static str> {
    let uid = unsafe { libc::getuid() };
    let mut buffer = vec![0_u8; 16 * 1024];
    let mut user = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result = ptr::null_mut();
    loop {
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                user.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return Err("Could not determine the session user for PAM authentication.");
        }
        let name = unsafe { (*result).pw_name };
        if name.is_null() {
            return Err("The session user has no login name.");
        }
        return Ok(unsafe { CStr::from_ptr(name) }.to_owned());
    }
}

fn has_service() -> bool {
    ["/etc/pam.d/astal-auth", "/usr/lib/pam.d/astal-auth"]
        .iter()
        .any(|path| Path::new(path).is_file())
}

pub fn check() -> Result<(), &'static str> {
    if !has_service() {
        return Err("Configure the astal-auth PAM service to enable the lock screen.");
    }
    Pam::open().map(drop)
}

pub fn verify(user: &CStr, password: &Password) -> bool {
    if !has_service() || password.0.len() <= 1 {
        return false;
    }
    let Ok(pam) = Pam::open() else { return false };
    let mut credentials = Credentials { user, password };
    let conv = Conversation {
        respond: conversation,
        data: ptr::from_mut(&mut credentials).cast(),
    };
    let mut transaction = ptr::null_mut();
    unsafe {
        let started = (pam.start)(SERVICE.as_ptr(), user.as_ptr(), &conv, &mut transaction);
        if started != SUCCESS || transaction.is_null() {
            return false;
        }
        let mut status = (pam.authenticate)(transaction, DISALLOW_NULL_TOKEN);
        if status == SUCCESS {
            status = (pam.account)(transaction, 0);
        }
        let ended = (pam.end)(transaction, status);
        status == SUCCESS && ended == SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_routes_password_and_login_name_to_their_respective_prompts() {
        let password = Password::new(c"test-password");
        let mut credentials = Credentials {
            user: c"test-user",
            password: &password,
        };
        let hidden = Message {
            style: PROMPT_HIDDEN,
            text: ptr::null(),
        };
        let visible = Message {
            style: PROMPT_VISIBLE,
            text: ptr::null(),
        };
        let info = Message {
            style: TEXT_INFO,
            text: ptr::null(),
        };
        let mut messages: [*const Message; 3] = [&hidden, &visible, &info];
        let mut response = ptr::null_mut();
        unsafe {
            let result = conversation(
                3,
                messages.as_mut_ptr(),
                &mut response,
                ptr::from_mut(&mut credentials).cast(),
            );
            assert_eq!(result, SUCCESS);
            assert_eq!(CStr::from_ptr((*response).text), c"test-password");
            assert_eq!(CStr::from_ptr((*response.add(1)).text), c"test-user");
            assert!((*response.add(2)).text.is_null());
            discard_responses(response, 3);
        }
    }

    #[test]
    fn conversation_discards_partial_responses_for_an_unsupported_prompt() {
        let password = Password::new(c"test-password");
        let mut credentials = Credentials {
            user: c"test-user",
            password: &password,
        };
        let hidden = Message {
            style: PROMPT_HIDDEN,
            text: ptr::null(),
        };
        let unsupported = Message {
            style: -1,
            text: ptr::null(),
        };
        let mut messages: [*const Message; 2] = [&hidden, &unsupported];
        let mut response = ptr::null_mut();
        let result = unsafe {
            conversation(
                2,
                messages.as_mut_ptr(),
                &mut response,
                ptr::from_mut(&mut credentials).cast(),
            )
        };
        assert_eq!(result, CONVERSATION_ERROR);
        assert!(response.is_null());
    }
}

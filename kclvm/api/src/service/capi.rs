use prost::Message;

use crate::gpyrpc::*;
use crate::service::service_impl::KclvmServiceImpl;
use std::ffi::CString;
use std::os::raw::c_char;
use std::slice;

#[allow(non_camel_case_types)]
type kclvm_service = KclvmServiceImpl;

fn c_char_to_vec(args: *const c_char, args_len: usize) -> Vec<u8> {
    if args.is_null() {
        return Vec::new();
    }
    // Create a slice from the raw pointer
    let slice = unsafe { slice::from_raw_parts(args as *const u8, args_len) };
    // Convert slice to Vec<u8>
    slice.to_vec()
}

/// Create an instance of kclvm_service and return its pointer
#[no_mangle]
pub extern "C-unwind" fn kclvm_service_new(plugin_agent: u64) -> *mut kclvm_service {
    let serv = kclvm_service { plugin_agent };
    Box::into_raw(Box::new(serv))
}

/// # Safety
///
/// This function should not be called twice on the same ptr.
/// Delete KclvmService
#[no_mangle]
pub unsafe extern "C-unwind" fn kclvm_service_delete(serv: *mut kclvm_service) {
    if !serv.is_null() {
        unsafe {
            drop(Box::from_raw(serv));
        }
    }
}

/// # Safety
///
/// This function should not be called twice on the same ptr.
/// Free memory for string returned to the outside
#[no_mangle]
pub unsafe extern "C-unwind" fn kclvm_service_free_string(res: *mut c_char) {
    if !res.is_null() {
        unsafe {
            let _ = CString::from_raw(res);
        };
    }
}

macro_rules! call {
    ($serv:expr, $args:expr, $args_len:expr, $result_len:expr, $arg_name:ident, $serv_name:ident) => {{
        unsafe {
            let serv_ref = &mut *$serv;
            let args = c_char_to_vec($args, $args_len);
            let args = args.as_slice();
            let args = $arg_name::decode(args).unwrap();
            let res = serv_ref.$serv_name(&args);
            let result_byte = match res {
                Ok(res) => res.encode_to_vec(),
                Err(err) => format!("ERROR:{}", err.to_string()).into_bytes(),
            };
            *$result_len = result_byte.len();
            CString::from_vec_unchecked(result_byte).into_raw()
        }
    }};
}

/// Call kclvm service by C API. **Note that it is not thread safe.**
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
/// `call`: [*const c_char]
///     The C str of the name of the called service,
///     with the format "KclvmService.{MethodName}"
///
/// `args`: [*const c_char]
///     Arguments of the call serialized as protobuf byte sequence,
///     refer to kclvm/spec/gpyrpc/gpyrpc.proto for the specific definitions of arguments
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
#[no_mangle]
pub extern "C-unwind" fn kclvm_service_call(
    serv: *mut kclvm_service,
    name: *const c_char,
    args: *const c_char,
    args_len: usize,
) -> *const c_char {
    let mut _result_len = 0;
    kclvm_service_call_with_length(serv, name, args, args_len, &mut _result_len)
}

/// Call kclvm service by C API. **Note that it is not thread safe.**
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
/// `call`: [*const c_char]
///     The C str of the name of the called service,
///     with the format "KclvmService.{MethodName}"
///
/// `args`: [*const c_char]
///     Arguments of the call serialized as protobuf byte sequence,
///     refer to kclvm/spec/gpyrpc/gpyrpc.proto for the specific definitions of arguments
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
#[no_mangle]
pub extern "C-unwind" fn kclvm_service_call_with_length(
    serv: *mut kclvm_service,
    name: *const c_char,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    let result = std::panic::catch_unwind(|| {
        let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_str().unwrap();
        let call = kclvm_get_service_fn_ptr_by_name(name);
        if call == 0 {
            panic!("null fn ptr");
        }
        let call = (&call as *const u64) as *const ()
            as *const fn(
                serv: *mut kclvm_service,
                args: *const c_char,
                args_len: usize,
                result_len: *mut usize,
            ) -> *const c_char;
        unsafe { (*call)(serv, args, args_len, result_len) }
    });
    match result {
        Ok(result) => result,
        Err(panic_err) => {
            let err_message = kclvm_error::err_to_str(panic_err);

            let c_string = std::ffi::CString::new(format!("ERROR:{}", err_message.as_str()))
                .expect("CString::new failed");
            let ptr = c_string.into_raw();
            ptr as *const c_char
        }
    }
}

pub(crate) fn kclvm_get_service_fn_ptr_by_name(name: &str) -> u64 {
    match name {
        "KclvmService.Ping" => ping as *const () as u64,
        "KclvmService.GetVersion" => get_version as *const () as u64,
        "KclvmService.ParseFile" => parse_file as *const () as u64,
        "KclvmService.ParseProgram" => parse_program as *const () as u64,
        "KclvmService.LoadPackage" => load_package as *const () as u64,
        "KclvmService.ListOptions" => list_options as *const () as u64,
        "KclvmService.ListVariables" => list_variables as *const () as u64,
        "KclvmService.ExecProgram" => exec_program as *const () as u64,
        #[cfg(feature = "llvm")]
        "KclvmService.BuildProgram" => build_program as *const () as u64,
        #[cfg(feature = "llvm")]
        "KclvmService.ExecArtifact" => exec_artifact as *const () as u64,
        "KclvmService.OverrideFile" => override_file as *const () as u64,
        "KclvmService.GetSchemaTypeMapping" => get_schema_type_mapping as *const () as u64,
        "KclvmService.GetSchemaTypeMappingUnderPath" => {
            get_schema_type_mapping_under_path as *const () as u64
        }
        "KclvmService.FormatCode" => format_code as *const () as u64,
        "KclvmService.FormatPath" => format_path as *const () as u64,
        "KclvmService.LintPath" => lint_path as *const () as u64,
        "KclvmService.ValidateCode" => validate_code as *const () as u64,
        "KclvmService.LoadSettingsFiles" => load_settings_files as *const () as u64,
        "KclvmService.Rename" => rename as *const () as u64,
        "KclvmService.RenameCode" => rename_code as *const () as u64,
        "KclvmService.Test" => test as *const () as u64,
        #[cfg(not(target_arch = "wasm32"))]
        "KclvmService.UpdateDependencies" => update_dependencies as *const () as u64,
        _ => panic!("unknown method name : {name}"),
    }
}

/// ping is used to test whether kclvm service is successfully imported
/// arguments and return results should be consistent
pub(crate) fn ping(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(serv, args, args_len, result_len, PingArgs, ping)
}

/// get_version is used to get kclvm service version
pub(crate) fn get_version(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        GetVersionArgs,
        get_version
    )
}

/// parse_file provides users with the ability to parse kcl single file
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn parse_file(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(serv, args, args_len, result_len, ParseFileArgs, parse_file)
}

/// parse_program provides users with the ability to parse kcl program
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn parse_program(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ParseProgramArgs,
        parse_program
    )
}

/// load_package provides users with the ability to parse kcl program and sematic model
/// information including symbols, types, definitions, etc,
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn load_package(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        LoadPackageArgs,
        load_package
    )
}

/// list_options provides users with the ability to parse kcl program and get all option
/// calling information.
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn list_options(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ParseProgramArgs,
        list_options
    )
}

/// list_variables provides users with the ability to parse kcl program and get all variables
/// calling information.
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn list_variables(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ListVariablesArgs,
        list_variables
    )
}

/// exec_program provides users with the ability to execute KCL code
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn exec_program(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ExecProgramArgs,
        exec_program
    )
}

/// build_program provides users with the ability to build the KCL program to an artifact.
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
#[cfg(feature = "llvm")]
pub(crate) fn build_program(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        BuildProgramArgs,
        build_program
    )
}

/// build_program provides users with the ability to execute the KCL artifact.
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
#[cfg(feature = "llvm")]
pub(crate) fn exec_artifact(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ExecArtifactArgs,
        exec_artifact
    )
}

/// override_file enable users override existing KCL file with specific KCl code
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     kcl file , override specs and import paths selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn override_file(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        OverrideFileArgs,
        override_file
    )
}

/// Get schema types from a kcl file or code.
///
/// # Parameters
/// file: [&str]. The kcl filename.
///
/// code: [Option<&str>]. The kcl code string
///
/// schema_name: [Option<&str>]. The schema name, when the schema name is empty, all schemas are returned.
pub(crate) fn get_schema_type_mapping(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        GetSchemaTypeMappingArgs,
        get_schema_type_mapping
    )
}

/// Get schema types under path
///
/// # Parameters
/// file: [&str]. The kcl filename.
///
/// code: [Option<&str>]. The kcl code string
///
/// schema_name: [Option<&str>]. The schema name, when the schema name is empty, all schemas are returned.
pub(crate) fn get_schema_type_mapping_under_path(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        GetSchemaTypeMappingArgs,
        get_schema_type_mapping_under_path
    )
}

/// Service for formatting a code source and returns the formatted source and
/// whether the source is changed.
pub(crate) fn format_code(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        FormatCodeArgs,
        format_code
    )
}

/// Service for formatting kcl file or directory path contains kcl files and
/// returns the changed file paths.
pub(crate) fn format_path(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        FormatPathArgs,
        format_path
    )
}

/// Service for KCL Lint API, check a set of files, skips execute,
/// returns error message including errors and warnings.
pub(crate) fn lint_path(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(serv, args, args_len, result_len, LintPathArgs, lint_path)
}

/// Service for validating the data string using the schema code string, when the parameter
/// `schema` is omitted, use the first schema appeared in the kcl code.
pub(crate) fn validate_code(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        ValidateCodeArgs,
        validate_code
    )
}

/// Service for building setting file config from args.
pub(crate) fn load_settings_files(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        LoadSettingsFilesArgs,
        load_settings_files
    )
}

/// Service for renaming all the occurrences of the target symbol in the files. This API will rewrite files if they contain symbols to be renamed.
/// return the file paths got changed.
pub(crate) fn rename(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(serv, args, args_len, result_len, RenameArgs, rename)
}

/// Service for renaming all the occurrences of the target symbol in the code. This API won't rewrite files but return the modified code if any code has been changed.
/// return the changed code.
pub(crate) fn rename_code(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        RenameCodeArgs,
        rename_code
    )
}

/// Service for the testing tool.
pub(crate) fn test(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(serv, args, args_len, result_len, TestArgs, test)
}

#[cfg(not(target_arch = "wasm32"))]
/// Service for the dependencies updating
/// calling information.
///
/// # Parameters
///
/// `serv`: [*mut kclvm_service]
///     The pointer of &\[[KclvmServiceImpl]]
///
///
/// `args`: [*const c_char]
///     the items and compile parameters selected by the user in the KCL CLI
///     serialized as protobuf byte sequence
///
/// # Returns
///
/// result: [*const c_char]
///     Result of the call serialized as protobuf byte sequence
pub(crate) fn update_dependencies(
    serv: *mut kclvm_service,
    args: *const c_char,
    args_len: usize,
    result_len: *mut usize,
) -> *const c_char {
    call!(
        serv,
        args,
        args_len,
        result_len,
        UpdateDependenciesArgs,
        update_dependencies
    )
}

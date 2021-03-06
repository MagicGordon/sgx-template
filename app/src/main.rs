// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License..

#![allow(dead_code)]
#![allow(unused_assignments)]
#![feature(proc_macro_hygiene, decl_macro)]



extern crate sgx_types;
extern crate sgx_urts;
use rocket::http::Accept;
use sgx_types::*;
use sgx_urts::SgxEnclave;

use std::convert::TryInto;
use std::os::unix::io::{IntoRawFd, AsRawFd};
use std::env;
use std::net::{TcpListener, TcpStream, SocketAddr};
use std::str;
use std::sync::{Arc, RwLock};

#[macro_use] extern crate lazy_static;

#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_contrib;
use rocket_contrib::json::{Json, JsonValue};

extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;


lazy_static! {
    static ref ENCLAVE: RwLock<Option<SgxEnclave>> = RwLock::new(None);
}

fn get_eid() -> u64 {
    ENCLAVE.read().unwrap().as_ref().unwrap().geteid()
}

fn destroy_enclave() {
    let enclave = ENCLAVE.write().unwrap().take().unwrap();
    enclave.destroy();
}

const BUFFER_SIZE: usize = 1024;

static ENCLAVE_FILE: &'static str = "enclave.signed.so";
static ENCLAVE_TOKEN: &'static str = "enclave.token";

extern {
    fn ecall_init(eid: sgx_enclave_id_t, retval: *mut sgx_status_t) -> sgx_status_t;

    fn ecall_action(
        eid: sgx_enclave_id_t, retval: *mut sgx_status_t,
        action: u8,
        input_ptr: *const u8, input_len: usize,
        output_ptr : *mut u8, output_len_ptr: *mut usize, output_buf_len: usize
    ) -> sgx_status_t;
}

#[no_mangle]
pub extern "C"
fn ocall_sgx_init_quote(ret_ti: *mut sgx_target_info_t,
                        ret_gid : *mut sgx_epid_group_id_t) -> sgx_status_t {
    println!("Entering ocall_sgx_init_quote");
    unsafe {sgx_init_quote(ret_ti, ret_gid)}
}


pub fn lookup_ipv4(host: &str, port: u16) -> SocketAddr {
    use std::net::ToSocketAddrs;

    let addrs = (host, port).to_socket_addrs().unwrap();
    for addr in addrs {
        if let SocketAddr::V4(_) = addr {
            return addr;
        }
    }

    unreachable!("Cannot lookup address");
}


#[no_mangle]
pub extern "C"
fn ocall_get_ias_socket(ret_fd : *mut c_int) -> sgx_status_t {
    let port = 443;
    let hostname = "api.trustedservices.intel.com";
    let addr = lookup_ipv4(hostname, port);
    let sock = TcpStream::connect(&addr).expect("[-] Connect tls server failed!");

    unsafe {*ret_fd = sock.into_raw_fd();}

    sgx_status_t::SGX_SUCCESS
}

#[no_mangle]
pub extern "C"
fn ocall_get_quote (p_sigrl            : *const u8,
                    sigrl_len          : u32,
                    p_report           : *const sgx_report_t,
                    quote_type         : sgx_quote_sign_type_t,
                    p_spid             : *const sgx_spid_t,
                    p_nonce            : *const sgx_quote_nonce_t,
                    p_qe_report        : *mut sgx_report_t,
                    p_quote            : *mut u8,
                    _maxlen             : u32,
                    p_quote_len        : *mut u32) -> sgx_status_t {
    println!("Entering ocall_get_quote");

    let mut real_quote_len : u32 = 0;

    let ret = unsafe {
        sgx_calc_quote_size(p_sigrl, sigrl_len, &mut real_quote_len as *mut u32)
    };

    if ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_calc_quote_size returned {}", ret);
        return ret;
    }

    println!("quote size = {}", real_quote_len);
    unsafe { *p_quote_len = real_quote_len; }

    let ret = unsafe {
        sgx_get_quote(p_report,
                      quote_type,
                      p_spid,
                      p_nonce,
                      p_sigrl,
                      sigrl_len,
                      p_qe_report,
                      p_quote as *mut sgx_quote_t,
                      real_quote_len)
    };

    if ret != sgx_status_t::SGX_SUCCESS {
        println!("sgx_calc_quote_size returned {}", ret);
        return ret;
    }

    println!("sgx_calc_quote_size returned {}", ret);
    ret
}

#[no_mangle]
pub extern "C"
fn ocall_get_update_info (platform_blob: * const sgx_platform_info_t,
                          enclave_trusted: i32,
                          update_info: * mut sgx_update_info_bit_t) -> sgx_status_t {
    unsafe{
        sgx_report_attestation_status(platform_blob, enclave_trusted, update_info)
    }
}

fn init_enclave() -> SgxResult<SgxEnclave> {
    let mut launch_token: sgx_launch_token_t = [0; 1024];
    let mut launch_token_updated: i32 = 0;
    // call sgx_create_enclave to initialize an enclave instance
    // Debug Support: set 2nd parameter to 1
    let debug = 1;
    let mut misc_attr = sgx_misc_attribute_t {secs_attr: sgx_attributes_t { flags:0, xfrm:0}, misc_select:0};
    SgxEnclave::create(ENCLAVE_FILE,
                       debug,
                       &mut launch_token,
                       &mut launch_token_updated,
                       &mut misc_attr)
}

#[derive(Debug, Serialize, Deserialize)]
struct Action {
    operation: u32,
}

const ENCLAVE_OUTPUT_BUF_MAX_LEN: usize = 32760 as usize; 
#[post("/action", format = "json", data = "<action>")]
fn index(action: Json<Action>) -> JsonValue {
    println!("{}", ::serde_json::to_string_pretty(&*action).unwrap());
    let eid = get_eid();
    let input_string = serde_json::to_string(&*action).unwrap();
    let mut return_output_buf: [u8; ENCLAVE_OUTPUT_BUF_MAX_LEN] = [0; ENCLAVE_OUTPUT_BUF_MAX_LEN];
    let mut output_len : usize = 0;
    let output_slice = &mut return_output_buf;
    let output_ptr = output_slice.as_mut_ptr();
    let output_len_ptr = &mut output_len as *mut usize;

    let mut retval = sgx_status_t::SGX_SUCCESS;
    let result = unsafe {
        ecall_action(
            eid, &mut retval,
            0,
            input_string.as_ptr(), input_string.len(),
            output_ptr, output_len_ptr, ENCLAVE_OUTPUT_BUF_MAX_LEN
        )
    };
    match result {
        sgx_status_t::SGX_SUCCESS => {},
        _ => {
            return json!({"ERROR": format!("[-] ECALL ecall_say_something Failed {}!", result.as_str())});
        }
    }

    
    let output_slice = unsafe { std::slice::from_raw_parts(output_ptr, output_len) };
    let output_value: serde_json::value::Value = serde_json::from_slice(output_slice).unwrap();
    json!(output_value)
}

fn main() {
    let enclave = match init_enclave() {
        Ok(r) => {
            println!("[+] Init Enclave Successful {}!", r.geteid());
            r
        },
        Err(x) => {
            println!("[-] Init Enclave Failed {}!", x.as_str());
            return;
        },
    };
    ENCLAVE.write().unwrap().replace(enclave);
    let mut retval = sgx_status_t::SGX_SUCCESS;
    let result = unsafe {
        ecall_init(get_eid(), &mut retval)
    };
    match result {
        sgx_status_t::SGX_SUCCESS => {
            println!("ecall_init call success!");
        },
        _ => {
            println!("[-] ecall_init Failed {}!", result.as_str());
            return;
        }
    }

    match retval {
        sgx_status_t::SGX_SUCCESS => {
            println!("ecall_init execute success!");
        },
        _ => {
            println!("[-] ecall_init Failed {}!", result.as_str());
            return;
        }
    }


    rocket::ignite().mount("/", routes![index]).launch();
    
    destroy_enclave();
    println!("enclave.destroy()!");
}

use std::ffi::CString;

use crate::{
    credential::{
        BLOCK_MODE_GCM, ENCRYPTION_PADDING_NONE, KEY_ALGORITHM_AES, PROVIDER, PURPOSE_DECRYPT,
        PURPOSE_ENCRYPT,
    },
    keystore::{KeyGenParameterSpecBuilder, KeyGenerator},
};
use android_log_sys::{__android_log_write, LogPriority};
use jni::{JNIEnv, JavaVM, objects::JObject};
use keyring::Entry;

// package io.crates.keyring
// import android.content.Context
// class KeyringTests {
//     companion object {
//         external fun runTests();
//     }
// }
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_crates_keyring_KeyringTests_00024Companion_runTests(
    env: JNIEnv,
    _class: JObject,
) {
    let testing = [
        ("golden_path", golden_path as fn(JavaVM)),
        ("corrupted_entry", corrupted_entry),
        ("concurrent_access", concurrent_access),
    ]
    .iter()
    .map(|(name, entry)| {
        let java_vm = env.get_java_vm().unwrap();
        (name, std::thread::spawn(move || entry(java_vm)))
    })
    .collect::<Vec<_>>();

    for (name, testing) in testing {
        if let Err(e) = testing.join() {
            let error = e.downcast_ref::<String>();
            let msg = format!("{name} error: {error:?}");
            let msg = CString::new(msg).unwrap();
            let tag = c"unit-test";
            unsafe {
                __android_log_write(LogPriority::ERROR as i32, tag.as_ptr(), msg.as_ptr());
            }
        }
    }
}

fn golden_path(_vm: JavaVM) {
    let entry1 = Entry::new("myservice", "myuser").unwrap();
    let entry2 = Entry::new("myservice", "myuser2").unwrap();
    let entry3 = Entry::new("myservice2", "myuser").unwrap();
    entry1.delete_credential().unwrap();
    entry2.delete_credential().unwrap();
    entry3.delete_credential().unwrap();

    entry1.set_password("test").unwrap();
    assert_eq!(entry1.get_password().unwrap(), "test");
    match entry2.get_password() {
        Err(keyring::Error::NoEntry) => {}
        x => panic!("unexpected result on entry2 get_password(): {x:?}"),
    };
    match entry3.get_password() {
        Err(keyring::Error::NoEntry) => {}
        x => panic!("unexpected result on entry3 get_password(): {x:?}"),
    };

    entry2.set_password("test2").unwrap();
    assert_eq!(entry2.get_password().unwrap(), "test2");

    entry3.set_password("test3").unwrap();
    assert_eq!(entry3.get_password().unwrap(), "test3");
}

fn corrupted_entry(vm: JavaVM) {
    let entry1 = Entry::new("corrupted", "myuser").expect("Entry::new");
    entry1.set_password("test").expect("set_password");

    // Force generating new key in order to corrupt entry
    {
        let mut env = vm.attach_current_thread().expect("attach_current_thread");
        let env = &mut env;

        let key_generator_spec =
            KeyGenParameterSpecBuilder::new(env, "corrupted", PURPOSE_DECRYPT | PURPOSE_ENCRYPT)
                .expect("KeyGenParameterSpecBuilder::new")
                .set_block_modes(env, &[BLOCK_MODE_GCM])
                .expect("set_block_modes")
                .set_encryption_paddings(env, &[ENCRYPTION_PADDING_NONE])
                .expect("set_encryption_paddings")
                .set_user_authentication_required(env, false)
                .expect("set_user_authentication_required")
                .build(env)
                .expect("build");
        let key_generator =
            KeyGenerator::get_instance(env, KEY_ALGORITHM_AES, PROVIDER).expect("get_instance");
        key_generator
            .init(env, key_generator_spec.into())
            .expect("init");
        key_generator.generate_key(env).expect("generate_key");
    }

    match entry1.get_password() {
        Err(keyring::Error::PlatformFailure(_)) => (),
        x => panic!("unexpected result on corrupted get_password(): {x:?}"),
    }

    let entry1 = Entry::new("corrupted", "myuser").expect("Entry::new");
    match entry1.get_password() {
        Err(keyring::Error::PlatformFailure(_)) => (),
        x => panic!("unexpected result on corrupted get_password(): {x:?}"),
    }

    entry1.set_password("reset").unwrap();
    assert_eq!(entry1.get_password().unwrap(), "reset");
}

fn concurrent_access(_vm: JavaVM) {
    let all = (0..64)
        .map(|_| {
            std::thread::spawn(|| {
                let entry = Entry::new("concurrent", "user").unwrap();
                entry.set_password("same").unwrap();
            })
        })
        .collect::<Vec<_>>();

    for t in all {
        t.join().unwrap();
    }

    let entry = Entry::new("concurrent", "user").unwrap();
    assert_eq!(entry.get_password().unwrap(), "same");
}

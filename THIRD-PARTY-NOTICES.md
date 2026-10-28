# Third-party notices

Lustro is MIT-licensed (see [LICENSE](LICENSE)). It ships with, or is built from, the third-party
works below, which keep their own licenses. This file is generated from `Cargo.lock` and
`app/ui/package-lock.json` by `node scripts/third-party-notices.mjs`.

## Shipped in the app

### Fonts

| Font | Copyright | License |
|---|---|---|
| Manrope | Copyright 2019 The Manrope Project Authors (https://github.com/sharanda/manrope) | SIL Open Font License 1.1 |
| JetBrains Mono | Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono) | SIL Open Font License 1.1 |

Bundled unmodified from the [Fontsource](https://fontsource.org) packages `@fontsource/manrope` and
`@fontsource/jetbrains-mono`. The design prototype (`design/prototype/vendor/fonts`) carries
copies of the same fonts with their license files. The full license text is at the end of this file.

### JavaScript

| Package | Version | License |
|---|---|---|
| [three.js](https://github.com/mrdoob/three.js) (`three`; also vendored in `design/prototype/vendor/three`) | 0.186.1 | MIT, Copyright © 2010-2026 three.js authors |
| [`@tauri-apps/api`](https://github.com/tauri-apps/tauri) | 2.12.4 | Apache-2.0 OR MIT |
| [Svelte](https://github.com/sveltejs/svelte) runtime (compiled into the app), with `clsx` and `esm-env` | 5.57.3 | MIT |

### Rust crates

The app is built on [Tauri](https://tauri.app) and the crates below, which are compiled into it.
The list covers every target platform in `Cargo.lock` (Windows, macOS, Linux and others), so each
build uses a subset. CI checks with `cargo deny` ([deny.toml](deny.toml)) that every license is
on the allowed list.

<details>
<summary><b>MIT OR Apache-2.0</b> (238)</summary>

android_system_properties 0.1.6, anyhow 1.0.104, arrayvec 0.7.8, async-broadcast 0.7.2, async-recursion 1.2.0, async-trait 0.1.92, base64 0.21.7, base64 0.23.1, bitflags 2.13.2, block-buffer 0.10.4, bumpalo 3.20.3, camino 1.2.6, cargo-platform 0.1.9, cc 1.10.0, cfg-expr 0.15.8, cfg-if 1.0.5, chrono 0.4.45, cookie 0.18.2, core-foundation 0.10.1, core-foundation 0.9.4, core-foundation-sys 0.8.7, core-graphics 0.25.0, core-graphics-types 0.2.0, cpufeatures 0.2.17, cpufeatures 0.3.1, crc32fast 1.5.3, crossbeam-channel 0.5.17, crossbeam-deque 0.8.8, crossbeam-epoch 0.9.21, crossbeam-utils 0.8.23, crypto-common 0.1.7, defmt 1.2.0, defmt-macros 1.2.0, defmt-parser 1.1.0, deranged 0.5.9, digest 0.10.7, dirs 7.0.0, dirs-sys 0.5.0, displaydoc 0.2.7, dtoa 1.0.11, dyn-clone 1.0.20, either 1.19.0, embed_plist 1.2.2, enumflags2 0.7.12, enumflags2_derive 0.7.12, erased-serde 0.4.11, errno 0.3.14, fdeflate 0.3.7, field-offset 0.3.6, find-msvc-tools 0.1.15, flate2 1.1.10, form_urlencoded 1.2.2, futures-channel 0.3.34, futures-core 0.3.34, futures-executor 0.3.34, futures-io 0.3.34, futures-macro 0.3.34, futures-sink 0.3.34, futures-task 0.3.34, futures-util 0.3.34, getrandom 0.2.17, getrandom 0.3.4, getrandom 0.4.3, glob 0.3.4, hashbrown 0.12.3, hashbrown 0.16.1, hashbrown 0.17.1, hashlink 0.11.1, heck 0.4.1, heck 0.5.0, hex 0.4.3, html5ever 0.39.0, http 1.5.0, httparse 1.10.1, iana-time-zone 0.1.65, iana-time-zone-haiku 0.1.2, idna 1.1.0, ipnet 2.12.2, itoa 1.0.18, jni 0.22.4, jni-macros 0.22.4, jni-sys 0.3.1, jni-sys 0.4.1, jni-sys-macros 0.4.1, js-sys 0.3.106, jsonptr 0.7.1, keyboard-types 0.8.3, libc 0.2.191, lock_api 0.4.14, log 0.4.34, markup5ever 0.39.0, mime 0.3.17, ndk 0.9.0, ndk-context 0.1.1, ndk-sys 0.6.0+11769913, num-conv 0.2.2, num-traits 0.2.19, once_cell 1.21.4, openssl-probe 0.2.2, ordered-stream 0.2.0, osakit 0.3.1, parking_lot 0.12.5, parking_lot_core 0.9.12, percent-encoding 2.3.2, pkg-config 0.3.34, png 0.18.1, powerfmt 0.2.1, ppv-lite86 0.2.21, proc-macro-crate 1.3.1, proc-macro-crate 2.0.2, proc-macro-crate 3.5.0, proc-macro-error 1.0.4, proc-macro-error-attr 1.0.4, proc-macro2 1.0.107, quote 1.0.47, rand 0.9.5, rand_chacha 0.9.0, rand_core 0.9.5, rayon 1.12.0, rayon-core 1.13.0, ref-cast 1.0.27, ref-cast-impl 1.0.27, regex 1.13.1, regex-automata 0.4.18, regex-syntax 0.8.11, reqwest 0.13.5, rustc_version 0.4.1, rustls-pki-types 1.15.1, rustls-platform-verifier 0.7.1, rustls-platform-verifier-android 0.2.0, rustversion 1.0.23, scopeguard 1.2.0, security-framework 3.7.0, security-framework-sys 2.17.0, semver 1.0.28, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_derive_internals 0.29.1, serde_json 1.0.154, serde_repr 0.1.21, serde_spanned 0.6.9, serde_spanned 1.1.2, serde_with 3.24.0, serde_with_macros 3.24.0, serde-untagged 0.1.9, serialize-to-javascript 0.1.2, serialize-to-javascript-impl 0.1.2, servo_arc 0.4.3, sha2 0.10.9, shlex 2.0.1, signal-hook-registry 1.4.8, simdutf8 0.1.5, siphasher 1.0.4, smallvec 1.16.4, socket2 0.6.5, softbuffer 0.4.8, stable_deref_trait 1.2.1, string_cache 0.9.0, string_cache_codegen 0.6.1, swift-rs 1.0.9, syn 1.0.109, syn 2.0.119, syn 3.0.9, system-configuration 0.7.0, system-configuration-sys 0.6.0, system-deps 6.2.2, tao-macros 0.1.4, tar 0.4.46, tempfile 3.27.0, tendril 0.5.1, thiserror 1.0.69, thiserror 2.0.21, thiserror-impl 1.0.69, thiserror-impl 2.0.21, time 0.3.55, time-core 0.1.9, time-macros 0.2.32, tokio-rustls 0.26.6, toml 0.8.2, toml 1.1.8+spec-1.1.0, toml_datetime 0.6.3, toml_datetime 1.1.2+spec-1.1.0, toml_edit 0.19.15, toml_edit 0.20.2, toml_edit 0.25.17+spec-1.1.0, toml_parser 1.1.5+spec-1.1.0, toml_writer 1.1.3+spec-1.1.0, tray-icon 0.25.1, typed-path 0.13.0, typeid 1.0.3, typenum 1.20.1, unicode-segmentation 1.13.3, url 2.5.8, wasm-bindgen 0.2.129, wasm-bindgen-futures 0.4.79, wasm-bindgen-macro 0.2.129, wasm-bindgen-macro-support 0.2.129, wasm-bindgen-shared 0.2.129, wasm-streams 0.5.0, web_atoms 0.2.6, web-sys 0.3.106, web-time 1.1.0, windows 0.62.2, windows_aarch64_gnullvm 0.42.2, windows_aarch64_gnullvm 0.52.6, windows_aarch64_msvc 0.42.2, windows_aarch64_msvc 0.52.6, windows_i686_gnu 0.42.2, windows_i686_gnu 0.52.6, windows_i686_gnullvm 0.52.6, windows_i686_msvc 0.42.2, windows_i686_msvc 0.52.6, windows_x86_64_gnu 0.42.2, windows_x86_64_gnu 0.52.6, windows_x86_64_gnullvm 0.42.2, windows_x86_64_gnullvm 0.52.6, windows_x86_64_msvc 0.42.2, windows_x86_64_msvc 0.52.6, windows-collections 0.3.2, windows-core 0.62.2, windows-future 0.3.2, windows-implement 0.60.2, windows-interface 0.59.3, windows-link 0.2.1, windows-numerics 0.3.1, windows-registry 0.6.1, windows-result 0.4.1, windows-strings 0.5.1, windows-sys 0.45.0, windows-sys 0.52.0, windows-sys 0.59.0, windows-sys 0.61.2, windows-targets 0.42.2, windows-targets 0.52.6, windows-threading 0.2.1, windows-version 0.1.7, xattr 1.7.0

</details>

<details>
<summary><b>MIT</b> (128)</summary>

ashpd 0.11.1, atk 0.18.2, atk-sys 0.18.2, block2 0.6.2, bytes 1.12.1, cairo-rs 0.18.5, cairo-sys-rs 0.18.2, cargo_metadata 0.19.2, cfb 0.14.0, cfg_aliases 0.2.2, combine 4.6.9, darling 0.24.1, darling_core 0.24.1, darling_macro 0.24.1, derive_more 2.1.1, derive_more-impl 2.1.1, dlib 0.5.3, dlopen2 0.8.2, dlopen2_derive 0.4.3, dom_query 0.28.0, embed-resource 3.0.12, endi 1.1.1, gdk 0.18.2, gdk-pixbuf 0.18.5, gdk-pixbuf-sys 0.18.0, gdk-sys 0.18.2, gdkwayland-sys 0.18.2, gdkx11 0.18.2, gdkx11-sys 0.18.2, generic-array 0.14.7, gio 0.18.4, gio-sys 0.18.1, glib 0.18.5, glib-macros 0.18.5, glib-sys 0.18.1, gobject-sys 0.18.0, gtk 0.18.2, gtk-sys 0.18.2, gtk3-macros 0.18.2, http-body 1.1.0, http-body-util 0.1.5, hyper 1.12.0, hyper-util 0.1.21, ico 0.6.0, infer 0.22.0, javascriptcore-rs 1.1.2, javascriptcore-rs-sys 1.1.1, libredox 0.1.25, libsqlite3-sys 0.36.0, memoffset 0.9.1, minisign-verify 0.3.0, mio 1.2.4, new_debug_unreachable 1.0.6, nix 0.31.3, objc2 0.6.5, objc2-encode 4.1.0, objc2-foundation 0.3.2, pango 0.18.3, pango-sys 0.18.0, phf 0.13.1, phf_codegen 0.13.1, phf_generator 0.13.1, phf_macros 0.13.1, phf_shared 0.13.1, plist 1.10.3, precomputed-hash 0.1.1, quick-xml 0.41.0, quick-xml 0.42.0, redox_syscall 0.5.18, redox_users 0.5.3, rfd 0.15.4, rsqlite-vfs 0.1.1, rusqlite 0.38.0, schannel 0.1.30, schemars 0.8.22, schemars 0.9.0, schemars 1.2.3, schemars_derive 0.8.22, simd-adler32 0.3.10, slab 0.4.12, soup3 0.5.0, soup3-sys 0.5.0, sqlite-wasm-rs 0.5.5, strsim 0.11.1, synstructure 0.14.0, tauri-winres 0.3.7, tokio 1.54.1, tokio-util 0.7.20, tower 0.5.3, tower-http 0.6.11, tower-layer 0.3.3, tower-service 0.3.3, tracing 0.1.44, tracing-attributes 0.1.31, tracing-core 0.1.36, trash 5.2.9, try-lock 0.2.5, uds_windows 1.2.1, urlencoding 2.1.3, urlpattern 0.6.0, version-compare 0.2.1, vswhom 0.1.0, vswhom-sys 0.1.3, want 0.3.2, wayland-backend 0.3.17, wayland-client 0.31.15, wayland-protocols 0.32.13, wayland-scanner 0.31.11, wayland-sys 0.31.11, webkit2gtk 2.0.2, webkit2gtk-sys 2.0.2, webview2-com 0.39.1, webview2-com-macros 0.8.1, webview2-com-sys 0.39.1, winnow 0.5.40, winnow 1.0.4, winreg 0.56.0, x11 2.21.0, x11-dl 2.21.0, zbus 5.19.0, zbus_macros 5.19.0, zbus_names 4.3.5, zcheapstr 1.1.0, zip 9.1.3, zmij 1.0.23, zvariant 5.15.1, zvariant_derive 5.15.1, zvariant_utils 4.2.0

</details>

<details>
<summary><b>Apache-2.0 OR MIT</b> (37)</summary>

atomic-waker 1.1.2, autocfg 1.5.1, bit-set 0.8.0, bit-vec 0.8.0, cargo_toml 1.0.1, ctor 1.0.13, equivalent 1.0.3, event-listener 5.4.2, event-listener-strategy 0.5.4, fastrand 2.5.0, futures-lite 2.6.1, idna_adapter 1.2.2, indexmap 1.9.3, indexmap 2.14.2, libappindicator 0.9.0, libappindicator-sys 0.9.0, muda 0.20.0, parking 2.2.1, pin-project-lite 0.2.17, portable-atomic 1.15.0, portable-atomic-util 0.2.8, rustc-hash 2.1.3, simd_cesu8 1.2.0, tauri 2.12.5, tauri-build 2.7.4, tauri-codegen 2.7.4, tauri-macros 2.7.4, tauri-plugin 2.7.2, tauri-plugin-updater 2.14.1, tauri-runtime 2.12.2, tauri-runtime-wry 2.12.3, tauri-utils 2.10.2, utf8_iter 1.0.4, uuid 1.29.0, window-vibrancy 0.8.1, wry 0.57.1, zeroize 1.9.1

</details>

<details>
<summary><b>MIT/Apache-2.0</b> (18)</summary>

bitflags 1.3.2, bs58 0.5.1, downcast-rs 1.2.1, fallible-iterator 0.3.0, fallible-streaming-iterator 0.1.9, filetime 0.2.29, foreign-types 0.5.0, foreign-types-macros 0.2.4, foreign-types-shared 0.3.1, ident_case 1.0.1, jni 0.21.1, json-patch 4.2.0, scoped-tls 1.0.1, vcpkg 0.2.15, version_check 0.9.5, winapi 0.3.9, winapi-i686-pc-windows-gnu 0.4.0, winapi-x86_64-pc-windows-gnu 0.4.0

</details>

<details>
<summary><b>Unicode-3.0</b> (18)</summary>

icu_collections 2.4.0, icu_locale_core 2.4.0, icu_normalizer 2.4.0, icu_normalizer_data 2.4.0, icu_properties 2.4.0, icu_properties_data 2.4.0, icu_provider 2.4.0, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.5, writeable 0.6.4, yoke 0.8.4, yoke-derive 0.8.4, zerofrom 0.1.8, zerofrom-derive 0.1.8, zerotrie 0.2.5, zerovec 0.11.9, zerovec-derive 0.11.7

</details>

<details>
<summary><b>Zlib OR Apache-2.0 OR MIT</b> (18)</summary>

bytemuck 1.25.2, dispatch2 0.3.1, objc2-app-kit 0.3.2, objc2-cloud-kit 0.3.2, objc2-core-data 0.3.2, objc2-core-foundation 0.3.2, objc2-core-graphics 0.3.2, objc2-core-image 0.3.2, objc2-core-location 0.3.2, objc2-core-text 0.3.2, objc2-exception-helper 0.1.1, objc2-io-surface 0.3.2, objc2-osa-kit 0.3.2, objc2-quartz-core 0.3.2, objc2-ui-kit 0.3.2, objc2-user-notifications 0.3.2, objc2-web-kit 0.3.2, tinyvec 1.13.3

</details>

<details>
<summary><b>Unlicense OR MIT</b> (9)</summary>

aho-corasick 1.1.5, byteorder 1.5.0, jiff 0.2.38, jiff-core 0.1.1, jiff-static 0.2.38, jiff-tzdb 0.1.9, jiff-tzdb-platform 0.1.3, memchr 2.8.3, winapi-util 0.1.11

</details>

<details>
<summary><b>Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT</b> (5)</summary>

linux-raw-sys 0.12.1, rustix 1.1.6, wasi 0.11.1+wasi-snapshot-preview1, wasip2 1.0.4+wasi-0.2.12, wit-bindgen 0.57.1

</details>

<details>
<summary><b>MPL-2.0</b> (5)</summary>

cssparser 0.37.0, cssparser-macros 0.7.1, dtoa-short 0.3.5, option-ext 0.2.0, selectors 0.38.0

</details>

<details>
<summary><b>Apache-2.0/MIT</b> (4)</summary>

cesu8 1.1.0, dbus 0.9.12, libdbus-sys 0.2.7, pollster 0.4.0

</details>

<details>
<summary><b>ISC</b> (4)</summary>

libloading 0.7.4, libloading 0.8.9, rustls-webpki 0.103.16, untrusted 0.9.0

</details>

<details>
<summary><b>Apache-2.0 OR ISC OR MIT</b> (3)</summary>

hyper-rustls 0.27.10, rustls 0.23.46, rustls-native-certs 0.8.5

</details>

<details>
<summary><b>BSD-3-Clause</b> (3)</summary>

alloc-no-stdlib 3.0.0, alloc-stdlib 0.3.0, subtle 2.6.1

</details>

<details>
<summary><b>Apache-2.0</b> (2)</summary>

sync_wrapper 1.0.2, tao 0.37.2

</details>

<details>
<summary><b>BSD-2-Clause OR Apache-2.0 OR MIT</b> (2)</summary>

zerocopy 0.8.63, zerocopy-derive 0.8.63

</details>

<details>
<summary><b>BSD-3-Clause OR MIT OR Apache-2.0</b> (2)</summary>

num_enum 0.7.6, num_enum_derive 0.7.6

</details>

<details>
<summary><b>CC0-1.0 OR MIT-0 OR Apache-2.0</b> (2)</summary>

constant_time_eq 0.4.2, dunce 1.0.5

</details>

<details>
<summary><b>MIT OR Apache-2.0 OR LGPL-2.1-or-later</b> (2)</summary>

r-efi 5.3.0, r-efi 6.0.0

</details>

<details>
<summary><b>MIT OR Apache-2.0 OR Zlib</b> (2)</summary>

raw-window-handle 0.6.2, static_vcruntime 3.0.0

</details>

<details>
<summary><b>MIT OR Zlib OR Apache-2.0</b> (2)</summary>

miniz_oxide 0.8.9, miniz_oxide 0.9.1

</details>

<details>
<summary><b>Unlicense/MIT</b> (2)</summary>

same-file 1.0.6, walkdir 2.5.0

</details>

<details>
<summary><b>Zlib</b> (2)</summary>

foldhash 0.2.0, zlib-rs 0.6.8

</details>

<details>
<summary><b>(MIT OR Apache-2.0) AND Unicode-3.0</b> (1)</summary>

unicode-ident 1.0.27

</details>

<details>
<summary><b>0BSD OR MIT OR Apache-2.0</b> (1)</summary>

adler2 2.0.1

</details>

<details>
<summary><b>Apache-2.0 / MIT</b> (1)</summary>

fnv 1.0.7

</details>

<details>
<summary><b>Apache-2.0 AND ISC</b> (1)</summary>

ring 0.17.14

</details>

<details>
<summary><b>Apache-2.0 AND MIT</b> (1)</summary>

dpi 0.1.2

</details>

<details>
<summary><b>Apache-2.0 WITH LLVM-exception</b> (1)</summary>

target-lexicon 0.12.16

</details>

<details>
<summary><b>BSD-3-Clause AND MIT</b> (1)</summary>

brotli 9.1.1

</details>

<details>
<summary><b>BSD-3-Clause/MIT</b> (1)</summary>

brotli-decompressor 6.2.0

</details>

<details>
<summary><b>CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception</b> (1)</summary>

blake3 1.8.7

</details>

<details>
<summary><b>CDLA-Permissive-2.0</b> (1)</summary>

webpki-root-certs 1.0.11

</details>

### Windows installer

- Built with [NSIS](https://nsis.sourceforge.io) (zlib/libpng License).
- Embeds the Microsoft Edge WebView2 Runtime bootstrapper, which Microsoft allows applications to
  redistribute under the [WebView2 distribution terms](https://developer.microsoft.com/microsoft-edge/webview2/).

## Build and test tools (not shipped)

These npm packages are used to build or test the interface and are not part of the app. Among
them, `lightningcss` (MPL-2.0) is the CSS minifier, and `@dimforge/rapier3d-compat` (Apache-2.0)
is only a type dependency of `@types/three`; neither ends up in the app.

<details>
<summary><b>MIT</b> (56)</summary>

@jridgewell/gen-mapping 0.3.13, @jridgewell/remapping 2.3.5, @jridgewell/resolve-uri 3.1.2, @jridgewell/sourcemap-codec 1.6.0, @jridgewell/trace-mapping 0.3.31, @oxc-project/types 0.155.0, @rolldown/pluginutils 1.0.1, @sveltejs/acorn-typescript 1.0.13, @sveltejs/load-config 0.2.3, @sveltejs/vite-plugin-svelte 7.3.1, @tsconfig/svelte 5.0.8, @tweenjs/tween.js 23.1.3, @types/chai 5.2.3, @types/deep-eql 4.0.2, @types/estree 1.0.9, @types/node 24.19.2, @types/stats.js 0.17.4, @types/three 0.186.0, @types/webxr 0.5.24, @vitest/mocker 5.0.5, @vitest/spy 5.0.5, acorn 8.19.0, assertion-error 2.0.1, chai 6.3.0, chokidar 4.0.3, deepmerge 4.3.1, devalue 5.9.4, es-module-lexer 2.3.2, esrap 2.4.0, estree-walker 3.0.3, fdir 6.5.0, fflate 0.8.3, fsevents 2.3.3, is-reference 3.0.3, locate-character 3.0.0, magic-string 0.30.21, meshoptimizer 1.1.1, mri 1.2.0, nanoid 3.3.20, obug 3.0.0, picomatch 4.0.7, postcss 8.5.32, readdirp 4.1.2, rolldown 1.2.15, sade 1.8.1, std-env 4.3.0, svelte-check 4.7.7, tinybench 6.2.2, tinyexec 1.3.1, tinyglobby 0.2.17, undici-types 7.24.6, vite 8.3.5, vitefu 1.1.3, vitest 5.0.5, why-is-node-running 3.2.1, zimmerframe 1.1.5

</details>

<details>
<summary><b>Apache-2.0 OR MIT</b> (12)</summary>

@tauri-apps/cli 2.12.4, @tauri-apps/cli-darwin-arm64 2.12.4, @tauri-apps/cli-darwin-x64 2.12.4, @tauri-apps/cli-linux-arm-gnueabihf 2.12.4, @tauri-apps/cli-linux-arm64-gnu 2.12.4, @tauri-apps/cli-linux-arm64-musl 2.12.4, @tauri-apps/cli-linux-riscv64-gnu 2.12.4, @tauri-apps/cli-linux-x64-gnu 2.12.4, @tauri-apps/cli-linux-x64-musl 2.12.4, @tauri-apps/cli-win32-arm64-msvc 2.12.4, @tauri-apps/cli-win32-ia32-msvc 2.12.4, @tauri-apps/cli-win32-x64-msvc 2.12.4

</details>

<details>
<summary><b>Apache-2.0</b> (6)</summary>

@dimforge/rapier3d-compat 0.12.0, aria-query 5.3.1, axobject-query 4.1.0, detect-libc 2.1.2, expect-type 1.4.0, typescript 6.0.3

</details>

<details>
<summary><b>BSD-3-Clause</b> (1)</summary>

source-map-js 1.2.2

</details>

<details>
<summary><b>ISC</b> (1)</summary>

picocolors 1.1.1

</details>

<details>
<summary><b>MPL-2.0</b> (1)</summary>

lightningcss 1.33.0

</details>

---

## SIL Open Font License 1.1 (Manrope, JetBrains Mono)

```
This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

---

## MIT License (three.js)

```
The MIT License

Copyright © 2010-2026 three.js authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

# Third-party software

The AMD ADL headers in `crates/amd-adl/vendor` are copyright (c) 2016–2022
Advanced Micro Devices, Inc. and distributed under the MIT license included
in each header. Upstream revision: `84c881cb2136f04287eb3af12de00a9e2867aa9e`.

ADL is loaded from the installed AMD display driver; its DLL is not distributed
with this app. AMD and Radeon are trademarks of Advanced Micro Devices, Inc.
This independent project is not affiliated with AMD. The project logo is original
and is covered by the project MIT license.

Tauri, Rust crates and frontend dependencies retain their respective licenses;
resolved versions are recorded in Cargo.lock and package-lock.json.

`vendor/glib` contains the gtk-rs Project Developers' MIT-licensed glib 0.18.5
with the upstream fix for RUSTSEC-2024-0429 backported and pointer-slice truncation
changed to clear slots before dropping removed elements. Its license and copyright
notices are included in that directory. See `vendor/glib/PATCH.md` for provenance
and the exact changes. This dependency is used by the Linux GTK3 stack only.

## AMD ADL header license

Copyright (c) 2016–2022 Advanced Micro Devices, Inc. All rights reserved.

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

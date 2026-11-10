# Third-party notices

This repository contains source code copied from, or derived from, the third-party
projects listed below. Each section names the project, where its code lives in this
repository, and reproduces the project's licence in full. Dependencies that are
fetched from crates.io at build time are not listed here; their licences are in
their own packages.

## sgp4 2.3.0

Source: <https://crates.io/crates/sgp4> (<https://github.com/neuromorphicsystems/sgp4>).
Copied to `crates/sgp4-portable/`, with its math calls routed through the `libm`
crate; `crates/sgp4-portable/README.md` lists every change.

```text
MIT License

Copyright (c) 2020 International Centre for Neuromorphic Systems

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

# Allowlisted Docker Image Digests

**Verification Date:** 2026-10-30  
**Sources:**
- Remote: `https://speeddemon.dev/allowlist.json`
- Organizer repo: `hackathon-raptors/speed-demon-2026` (`harness/allowlist.json`)
- Starter repo: `README.md` Dockerfile example
- Team plan: `PLAN.md` section 5

## Verified Core Images

| Image | Multi-arch Digest | Local Pull Status | Use / Toolchain |
|---|---|---|---|
| `rust:1-slim-trixie` | `sha256:1a939748a7cbab172de91a014e3804a97af1394a8c844faa67fce9401c4e40fb` | Pulled & Verified | Rust 1.99.1 release builder (allowlist & PLAN.md match) |
| `debian:13-slim` | `sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad` | Pulled & Verified | Runtime minimal container stage (allowlist & PLAN.md match) |
| `python:3.13-slim` | `sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962` | Pulled & Verified | Python reference starter images and generators |
| `halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools` | `sha256:13e4ee7135d8eac6c56476c7d1aee201f36479f4745fd5d0e6b4529f4e13251a` | Pulled & Verified | Rust 1.99.1 + llvm-tools for PGO profiling |

## Complete Allowlist (as of 2026-10-30)

| Image | Digest | Use |
|---|---|---|
| `debian:13-slim` | `sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad` | runtime stage |
| `python:3.13-slim` | `sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962` | Python (the reference) |
| `rust:1-slim-trixie` | `sha256:1a939748a7cbab172de91a014e3804a97af1394a8c844faa67fce9401c4e40fb` | Rust |
| `rust:1-trixie` | `sha256:931c2c0531a8b143e044f0f3914dfe802f15bb3d1955f0b657e9359327ac2ffc` | Rust, full toolchain |
| `halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools` | `sha256:13e4ee7135d8eac6c56476c7d1aee201f36479f4745fd5d0e6b4529f4e13251a` | Rust, rust:1-trixie plus llvm-tools (llvm-profdata for PGO) |
| `golang:1-trixie` | `sha256:fbb7ae40b62ff8afe31662df570d39b4f5052ac32b90dae8e60d7657a20a74f4` | Go |
| `gcc:15` | `sha256:eb5e64286ad47187b32bda75c6b6916166ecb41da56559b8b09e0001748d2cce` | C and C++ |
| `gcc:16` | `sha256:43978d7710a803d1c3d32f4117c1528ec7285a4ce2c0a51e982ec1a6e3d87d95` | C and C++ |
| `eclipse-temurin:25-jdk` | `sha256:2d5d5e9b533342a565907c84f47a2ec298c40cde7b13a3ca85d4950fa456a641` | Java, Kotlin, Scala build |
| `eclipse-temurin:25-jre` | `sha256:dbb43d81099eec91e0800226ed2296d03146ee7857165b95112372ad0d67e87f` | JVM runtime stage |
| `ghcr.io/graalvm/native-image-community:25` | `sha256:0d936f32bb8acb5bc60c41b33e05f064d7a6aaf36b726538296c54949bd4a3c0` | GraalVM native-image |
| `node:22-slim` | `sha256:de170b39439eb0e5a3444ad7d60b015614a381e10fecae2e30ec9e735122f38d` | JavaScript and TypeScript |
| `node:24-slim` | `sha256:55fb5f5a527ea2c5c4a95f963e48ef90aa38c7b346054fdc8ee9957a2b55d205` | JavaScript and TypeScript |
| `mcr.microsoft.com/dotnet/sdk:10.0` | `sha256:747cbade5bfd918a175ac32f8f1d931edec2c7d0d5397a3de72eaef1ac1f66d2` | C# and F# build |
| `mcr.microsoft.com/dotnet/runtime:10.0` | `sha256:0cf09aaccf6a9f68900aed4c73fb28fe25bb59ec85de4dea08ba02240a8635f3` | .NET runtime stage |
| `swift:6.2` | `sha256:d6f8f6112e9a63da0a4a151d6b93acd16699d35c0b82531ac1a245f5af210197` | Swift |
| `haskell:9` | `sha256:15aeb965021bf51853f95a2328291d169aa541c836ca76986b97297df817f293` | Haskell |

## Rule Check
- All `FROM` directives in custom Dockerfiles MUST use one of the allowlisted images pinned by exact digest `@sha256:...`.
- No URLs or unpinned tags permitted.

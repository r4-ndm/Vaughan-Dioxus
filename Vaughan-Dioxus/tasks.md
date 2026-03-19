# Tasks: Dioxus + Tauri Multi-Chain Wallet

**Development follows this task list in order.** Check off items as they are completed.

## Task 1: Project Setup and Foundation

- [x] 1.1 Create new Rust project with Cargo workspace structure
- [x] 1.2 Set up module structure: chains/, core/, security/, gui/, models/, error/
- [x] 1.3 Configure Cargo.toml with core dependencies (alloy, tokio, serde, tracing, thiserror, dioxus, dioxus-desktop, dioxus-mobile)
- [x] 1.4 Configure optional feature flags: audio, qr, hardware-wallets, telemetry, mobile
- [x] 1.5 Create basic error types in error/mod.rs
- [x] 1.6 Set up logging with tracing and tracing-subscriber
- [x] 1.7 Run initial cargo audit; configure cargo-deny
- [ ]* 1.8 Add a local test dApp HTML page for EIP-1193/6963 testing

## Task 2: Core Data Models and Types

- [x] 2.1 Define shared types in models/mod.rs (AccountId, NetworkId, Balance, TxHash, TxRecord, TxStatus, Fee, TokenInfo)
- [x] 2.2 Define chain-agnostic types in chains/types.rs (ChainType, ChainInfo, Signature)
- [x] 2.3 Define error types in error/mod.rs (WalletError with all variants)

## Task 3: Chain Adapter Layer

- [x] 3.1 Implement ChainAdapter trait in chains/mod.rs
- [x] 3.2 Implement EVM adapter foundation in chains/evm/mod.rs
- [x] 3.3 Implement EVM network configurations in chains/evm/networks.rs (Ethereum, PulseChain, Polygon, Arbitrum, Optimism)
- [x] 3.4 Implement EVM balance operations in chains/evm/adapter.rs
- [ ]* 3.5 Write property test for balance queries (Property 8)
- [x] 3.6 Implement EVM transaction operations
- [ ]* 3.7 Write property test for transaction broadcasting (Property 9)
- [x] 3.8 Implement EVM gas estimation with EIP-1559 support
- [ ]* 3.9 Write property test for gas estimation (Property 10)
- [x] 3.10 Implement address validation in chains/evm/utils.rs
- [ ]* 3.11 Write unit tests for EVM adapter

## Task 4: Checkpoint – Chain Adapter Layer Complete

- [x] 4.1 Ensure all chain adapter tests pass
- [ ] 4.2 Review with user if questions arise

## Task 5: Security Layer Implementation

- [x] 5.1 Implement HD wallet in security/hd_wallet.rs (BIP-39/BIP-32/BIP-44)
- [ ]* 5.2 Write property test for HD wallet derivation (Property 1)
- [x] 5.3 Implement encryption in security/encryption.rs (AES-256-GCM + Argon2id)
- [ ]* 5.4 Write property test for encryption round-trip (Property 7)
- [x] 5.5 Implement OS keychain integration in security/keyring_service.rs
- [ ]* 5.6 Write property test for keyring storage (Property 6)
- [ ]* 5.7 Write unit tests for security layer

## Task 6: Core Wallet State Implementation

- [x] 6.1 Implement Account model in core/account.rs
- [ ]* 6.2 Write property test for account import round-trip (Property 2)
- [x] 6.3 Implement WalletState in core/wallet.rs
- [ ]* 6.4 Write property test for wallet lock state (Property 4)
- [x] 6.5 Implement account management in core/account.rs (create, import, export, rename, delete)
- [ ]* 6.6 Write unit tests for account management

## Task 7: Transaction Management Implementation

- [x] 7.1 Implement transaction types in core/transaction.rs
- [ ]* 7.2 Write property test for transaction validation (Property 12)
- [x] 7.3 Implement transaction signing
- [ ]* 7.4 Write property test for transaction signatures (Property 3)
- [x] 7.5 Implement nonce management
- [ ]* 7.6 Write property test for nonce sequentiality (Property 13)
- [x] 7.7 Implement transaction broadcasting
- [ ]* 7.8 Write property test for transaction status (Property 18)
- [x] 7.9 Implement token transfers (ERC-20)
- [ ]* 7.10 Write property test for token transfers (Property 17)
- [x] 7.11 Implement transaction history retrieval and caching
- [ ]* 7.12 Write property test for transaction history caching (Property 16)

## Task 8: Checkpoint – Core Layer Complete

- [x] 8.1 Ensure all core layer tests pass
- [ ] 8.2 Review with user if questions arise

## Task 9: Network Management Implementation

- [x] 9.1 Implement network management in core/network.rs
- [ ]* 9.2 Write property test for network switching (Property 11)
- [x] 9.3 Implement network health monitoring
- [ ]* 9.4 Write unit tests for network management

## Task 10: Data Persistence Implementation

- [x] 10.1 Implement storage layer in core/persistence.rs
- [ ]* 10.2 Write property test for account metadata persistence (Property 5)
- [ ]* 10.3 Write property test for keystore persistence (Property 19)
- [ ]* 10.4 Write property test for network configuration persistence (Property 20)
- [ ]* 10.5 Write property test for token configuration persistence (Property 21)
- [ ]* 10.6 Write property test for data integrity validation (Property 22)
- [ ]* 10.7 Write unit tests for persistence layer

## Task 11: Checkpoint – Persistence Layer Complete

- [x] 11.1 Ensure all persistence tests pass
- [ ] 11.2 Review with user if questions arise

## Task 12: Balance Monitoring System

- [x] 12.1 Implement balance watcher in monitoring/balance_watcher.rs
- [ ]* 12.2 Write property test for balance polling (Property 27)
- [ ]* 12.3 Write property test for balance change detection (Property 28)
- [ ]* 12.4 Write property test for balance history (Property 29)
- [x] 12.5 Implement exponential backoff for RPC errors
- [ ]* 12.6 Write unit tests for balance watcher

## Task 13: Audio Notification System (Optional)

- [ ]* 13.1 Implement audio notifier in audio/notifications.rs (behind audio feature flag)
- [ ]* 13.2 Write unit tests for audio notifications

## Task 14: Dioxus GUI Foundation

- [x] 14.1 Set up Dioxus for desktop and mobile (dioxus-desktop, dioxus-mobile dependencies; desktop and mobile entry points)
- [ ] 14.2 Configure mobile build targets (Android/iOS) with required permissions (**DEFERRED**: ship desktop first; Android first later, then iOS)
- [x] 14.3 Define main WalletApp component with Dioxus state management (use_signal, use_context)
- [x] 14.4 Define AppView enum and routing logic
- [x] 14.5 Implement shared theme system (colors, fonts, spacing) for wallet and browser
- [x] 14.6 Integrate WalletState with GUI via use_context_provider
- [x] 14.7 Set up use_coroutine for async backend operations (signing, balance fetching)

## Task 15: Dioxus Views Implementation

- [x] 15.1 Implement Dashboard view (account overview, balance display, quick action buttons, balance subscription)
- [x] 15.2 Implement Send transaction view (form, real-time validation, confirmation modal)
- [x] 15.3 Implement Receive view (address display, QR code behind qr feature, clipboard copy)
- [x] 15.4 Implement Transaction history view (list, status, filtering, search)
- [x] 15.5 Implement Settings view (network management, account management, preferences, security)
- [x] 15.6 Implement Import/Export view (private key and mnemonic import, export with security warnings)

## Task 16: Dioxus Widgets and Components

- [x] 16.1 Create reusable widgets: AddressDisplay, BalanceDisplay, TxStatusBadge, NetworkSelector
- [x] 16.2 Implement clipboard operations (wasm-clipboard for web/mobile, native on desktop)
- [x] 16.3 Implement real-time balance watcher integration via Dioxus coroutine
- [x] 16.4 Implement transaction status polling subscription

## Task 17: Checkpoint – GUI Layer Complete

- [x] 17.1 Ensure Dioxus app builds and runs on desktop
- [ ]* 17.2 Verify app runs on Android and iOS simulators
- [ ] 17.3 Review with user if questions arise

## Task 18: Hardware Wallet Integration (Optional)

- [ ]* 18.1 Implement hardware wallet support in security/hardware/ (behind hardware-wallets feature)
- [ ]* 18.2 Implement Ledger and Trezor device operations (connect, derive address, sign)
- [ ]* 18.3 Implement signing request routing with Dioxus confirmation UI
- [ ]* 18.4 Write unit tests for hardware wallet integration

## Task 19: Token Management

- [x] 19.1 Implement token management in core/token.rs
- [ ]* 19.2 Write property test for token management (Property 26)
- [x] 19.3 Add token UI in Dioxus (list tokens, add custom token, remove token)

## Task 20: Performance Optimizations

- [x] 20.1 Implement RPC response caching with moka (balance 10s TTL, gas price 15s TTL, nonce 5s TTL)
- [x] 20.2 Implement HTTP connection pooling for RPC requests
- [x] 20.3 Implement CPU-intensive operation offloading via tokio::task::spawn_blocking
- [ ]* 20.4 Run performance benchmarks (signing, balance query, keystore unlock)

## Task 21: Error Handling and Recovery

- [ ] 21.1 Implement comprehensive error handling across all layers
- [ ] 21.2 Implement error recovery mechanisms (retry with backoff, user-friendly messages)
- [ ]* 21.3 Write unit tests for error handling

## Task 22: Security Hardening

- [ ] 22.1 Implement password validation (strength requirements)
- [ ]* 22.2 Write property test for password validation (Property 23)
- [ ] 22.3 Implement authentication rate limiting (lockout after 5 failed attempts)
- [ ]* 22.4 Write property test for rate limiting (Property 24)
- [ ] 22.5 Complete security audit checklist

## Task 23: Checkpoint – Security Hardening Complete

- [ ] 23.1 Ensure all security tests pass
- [ ] 23.2 Review with user if questions arise
- [ ]* 23.3 Replace placeholder `vaughan-tauri-browser/icons/icon.ico` with a real Windows `.ico` before release

## Task 24: Testing and Quality Assurance

- [ ] 24.1 Achieve minimum 80% code coverage for core modules
- [ ]* 24.2 Run all property tests with increased iterations (1000 per property)
- [ ]* 24.3 Integration testing (end-to-end: create account → send tx → verify confirmation)
- [ ]* 24.4 Security testing (verify no key logging, zeroization, Argon2id params, AES-GCM nonces)

## Task 25: Documentation

- [ ] 25.1 Write architecture documentation (layered design, module responsibilities, Dioxus vs Iced comparison)
- [ ] 25.2 Write API documentation (rustdoc for all public APIs)
- [ ] 25.3 Write developer guide (setup, build, test, mobile targets, contributing)
- [ ] 25.4 Write user documentation (installation, usage, security best practices)

## Task 26: Build Configuration and Optimization

- [ ] 26.1 Configure release build optimizations (opt-level="z", LTO, codegen-units=1, strip, panic="abort")
- [ ]* 26.2 Add Android build target (cargo-ndk, AndroidManifest.xml permissions)
- [ ]* 26.3 Add iOS build target (Xcode project, entitlements, cargo bundle)
- [ ] 26.4 Verify binary size targets (desktop < 20MB stripped, minimal < 15MB, full < 25MB)

## Task 27: CI/CD and Release Preparation

- [ ] 27.1 Set up GitHub Actions CI (build all platforms, tests, clippy, rustfmt, cargo-audit)
- [ ] 27.2 Configure code signing (Windows signtool, macOS codesign + notarization)
- [ ] 27.3 Create platform installers (Windows .msi, macOS .dmg, Linux .AppImage)
- [ ]* 27.4 Add Android and iOS build steps to CI (cargo-ndk, Xcode)
- [ ] 27.5 Set up auto-updater (self_update crate, update check on startup, notification UI)
- [ ] 27.6 Prepare release checklist (CHANGELOG.md, version management, release process docs)

## Task 28: Platform-Specific Integration

- [ ] 28.1 Windows platform integration (Credential Manager, %APPDATA% paths, signtool, AppUserModelID)
- [ ] 28.2 macOS platform integration (Keychain, ~/Library/ paths, notarization, universal binary)
- [ ] 28.3 Linux platform integration (Secret Service API, XDG paths, AppImage, WM_CLASS)

## Task 29: dApp Browser Foundation – IPC Types and Protocol

- [ ] 29.1 Create vaughan-ipc-types shared crate with IpcRequest, IpcResponse, Handshake, SignTxPayload types
- [ ] 29.2 Implement IPC message validation (address format, value parsing, chain ID matching)
- [ ]* 29.3 Write unit tests for IPC types (serialization round-trip, validation)

## Task 30: dApp Browser – Tauri Host Application

- [x] 30.1 Create new Tauri project (vaughan-tauri-browser) with dependencies: tauri, dioxus-desktop, interprocess, vaughan-ipc-types
- [x] 30.2 Configure tauri.conf.json (window title, size, CSP, macOS LSUIElement)
- [x] 30.3 Implement IPC client in Tauri backend (parse CLI args, connect to wallet, perform handshake)
- [x] 30.4 Spawn background task to forward messages between Tauri commands and IPC stream
- [x] 30.5 Build dApp browser UI with Dioxus inside Tauri (address bar, back/forward, refresh, connection status)
- [x] 30.6 Apply shared theme to browser UI
- [x] 30.7 Implement EIP-1193 provider injection script (window.ethereum, request, on, removeListener)
- [x] 30.8 Implement EIP-6963 wallet discovery protocol (announceProvider, requestProvider events)
- [x] 30.9 Implement all required EIP-1193 JSON-RPC methods (eth_requestAccounts, eth_sendTransaction, personal_sign, eth_signTypedData_v4, wallet_switchEthereumChain, etc.)
- [x] 30.10 Implement all required EIP-1193 events (connect, disconnect, accountsChanged, chainChanged)
- [x] 30.11 Implement EIP-1193 standard error codes (4001, 4100, 4200, 4900, -32xxx)
- [ ]* 30.12 Write unit tests for Tauri browser components

## Task 31: dApp Browser – Wallet Integration (IPC Server)

- [ ] 31.1 Modify Dioxus wallet to launch Tauri browser as child process (std::process::Command with IPC endpoint and token)
- [ ] 31.2 Track child process handle; kill browser on wallet exit
- [ ] 31.3 Implement IPC server in Dioxus wallet (interprocess LocalSocketListener, handshake validation, message handling task)
- [ ] 31.4 Extend Dioxus state with DappRequest and DappResponse handling
- [ ] 31.5 Implement approval modals in Dioxus for transaction signing requests
- [ ] 31.6 Implement message signing approval flow
- [ ] 31.7 Manage browser process lifecycle (health monitoring, auto-restart on crash)
- [ ]* 31.8 Write unit tests for IPC integration in Dioxus wallet

## Task 32: dApp Browser – Window Positioning and Visual Cohesion

- [ ] 32.1 Position browser window relative to main wallet window on launch (Tauri WindowBuilder with coordinates)
- [ ] 32.2 Implement Windows AppUserModelID for taskbar grouping
- [ ] 32.3 Implement macOS helper process configuration (LSUIElement, no Dock icon)
- [ ] 32.4 Implement Linux WM_CLASS for window manager grouping
- [ ] 32.5 Emit browser lifecycle events back to wallet via IPC (BrowserEvent: WindowClosed, WindowFocused)

## Task 33: dApp Browser – Security and Validation

- [ ] 33.1 Implement shared secret token authentication for IPC connection
- [ ] 33.2 Implement request validation in wallet (address format, value, chain ID)
- [ ] 33.3 Log all security-relevant dApp interactions (no sensitive data)
- [ ] 33.4 Enforce process isolation (browser in separate OS process)
- [ ]* 33.5 Write security tests for IPC authentication and request validation
- [ ] 33.6 Document the multi-process security model for users

## Task 34: dApp Browser – Distribution and Packaging

- [ ] 34.1 Configure Cargo workspace to include both vaughan-dioxus and vaughan-tauri-browser
- [ ] 34.2 Configure cargo-bundle to package both executables into a single installer
- [ ] 34.3 Implement browser executable discovery in wallet (same dir → standard locations → PATH)
- [ ] 34.4 Create platform-specific installers (Windows .msi, macOS .dmg, Linux .deb/.rpm/.AppImage)
- [ ] 34.5 Configure installer metadata, icons, and descriptions
- [ ]* 34.6 Write integration tests for distribution (verify both executables present and launchable)

## Task 35: dApp Browser – Testing and Integration

- [ ]* 35.1 Write IPC integration tests (handshake, GetAccounts flow, SignTransaction flow)
- [ ] 35.2 Create local test dApp page (test-dapp/index.html) for EIP-1193/6963 verification
- [ ]* 35.3 Write end-to-end dApp interaction tests
- [ ]* 35.4 Test EIP-1193 and EIP-6963 compliance with external dApps (Uniswap, etc.)
- [ ]* 35.5 Performance testing (IPC latency, memory usage under load)

## Task 36: Checkpoint – dApp Browser Integration Complete

- [ ] 36.1 Ensure all dApp browser tests pass
- [ ] 36.2 Review with user if questions arise

## Task 37: Final Integration and Wiring

- [ ] 37.1 Wire all components in Dioxus wallet main function (logging, WalletState, balance watcher, IPC server, GUI)
- [ ] 37.2 Implement application lifecycle (startup unlock flow, graceful shutdown, browser termination)
- [ ]* 37.3 Integrate Sentry error reporting (behind telemetry feature flag; scrub keys/mnemonics/addresses, opt-in toggle)
- [ ] 37.4 Final end-to-end testing (account creation, import, send tx, network switch, lock/unlock, balance monitoring, dApp browser interaction)

## Task 38: Final Checkpoint – Complete Application

- [ ] 38.1 Ensure all tests pass across all modules
- [ ] 38.2 Verify binary size targets met
- [ ] 38.3 Review with user; confirm ready for release

---

*Tasks marked with `*` are optional for MVP.*

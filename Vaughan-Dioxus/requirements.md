# Requirements Document: Dioxus + Tauri Multi-Chain Wallet

## Introduction

This document specifies the requirements for building a new Dioxus-based multi-chain wallet from scratch. The implementation reuses the same wallet core (chain adapters, security, state management, transaction logic) as the Iced-based design, but replaces the GUI layer with Dioxus for cross-platform desktop and mobile support, and replaces the Dioxus dApp browser with a Tauri + Dioxus dApp browser running as a separate isolated process.

Both existing codebases (Vaughan-Iced and Vaughan-Tauri) serve as reference implementations. This is a greenfield project, not a migration.

## Glossary

- **Vaughan_Iced**: Reference implementation - production-ready Ethereum wallet using the Iced GUI framework
- **Vaughan_Tauri**: Reference implementation - Tauri 2.0-based wallet with trait-based multi-chain architecture
- **New_Wallet**: The new Dioxus-based multi-chain wallet being built from scratch
- **Backend**: The Rust core wallet functionality including account management, transaction signing, key storage, and blockchain operations
- **ChainAdapter**: The trait-based abstraction (from Vaughan-Tauri reference) that enables multi-chain support
- **WalletService**: The core wallet service that manages accounts and signing operations
- **KeyringService**: The service that manages encrypted key storage and OS keychain integration
- **NetworkManager**: The component responsible for managing blockchain network connections and operations
- **Alloy**: The pure Rust Ethereum library used for blockchain operations
- **GUI_Layer**: The Dioxus-based user interface layer (desktop + mobile)
- **dApp_Browser**: The separate Tauri + Dioxus process that hosts a WebView for dApp interaction

## Requirements

### Requirement 1: Modular Project Structure and Architecture Setup

**User Story:** As a developer, I want to establish a clean modular project structure with clear separation of concerns, so that I can build a maintainable multi-chain wallet following the Tauri backend's architecture.

#### Acceptance Criteria

1. THE New_Wallet SHALL use a modular directory structure with the following modules:
   - `chains/` - Chain adapters (trait-based multi-chain support)
   - `core/` - Chain-agnostic wallet business logic
   - `security/` - Security-critical functionality (keyring, encryption, HD wallet)
   - `gui/` - Dioxus GUI layer
   - `models/` - Shared data types and structures
   - `error/` - Error types and handling
2. THE New_Wallet SHALL implement a trait-based ChainAdapter architecture (from Vaughan-Tauri reference)
3. THE New_Wallet SHALL maintain clear API boundaries between modules
4. THE New_Wallet SHALL use Cargo workspace structure for logical component separation
5. THE New_Wallet SHALL document each module's purpose and responsibilities in module-level README.md files
6. THE New_Wallet SHALL ensure no circular dependencies between modules
7. THE New_Wallet SHALL follow the layered architecture pattern:
   - Layer 2: GUI (Dioxus) - Presentation and user interaction, desktop and mobile
   - Layer 1: Core - Chain-agnostic business logic
   - Layer 0: Chain Adapters - Chain-specific operations

### Requirement 2: Wallet Core Implementation

**User Story:** As a developer, I want to implement a robust chain-agnostic wallet core, so that the wallet can manage accounts and operations across multiple chains.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement a core module in `core/` with the following components:
   - `wallet.rs` - WalletState managing all chain adapters
   - `account.rs` - Multi-chain account management
   - `transaction.rs` - Chain-agnostic transaction logic
   - `network.rs` - Network management across chains
   - `persistence.rs` - Data persistence and storage
2. THE New_Wallet SHALL implement WalletState that coordinates all chain adapters
3. THE New_Wallet SHALL support account creation with HD wallet derivation (BIP-39/BIP-32)
4. THE New_Wallet SHALL support account import via private key and mnemonic phrase
5. THE New_Wallet SHALL support account export functionality with security warnings
6. THE New_Wallet SHALL implement transaction signing for multiple account types
7. THE New_Wallet SHALL implement password-based wallet locking and unlocking
8. THE New_Wallet SHALL maintain account metadata (names, derivation paths, account types)
9. THE New_Wallet SHALL provide async API compatible with Dioxus GUI patterns (use_coroutine, use_future)
10. THE New_Wallet SHALL ensure all core code uses ONLY the ChainAdapter trait (chain-agnostic)
11. THE New_Wallet SHALL document the core architecture and design principles in `core/README.md`

### Requirement 3: Keystore and Security Implementation

**User Story:** As a developer, I want to implement secure key storage and management in a dedicated security module, so that user private keys are protected using industry best practices.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement a security module in `security/` with the following components:
   - `keyring_service.rs` - OS keychain integration
   - `encryption.rs` - Password-based encryption (AES-GCM + Argon2)
   - `hd_wallet.rs` - BIP-39/BIP-32 HD wallet implementation
2. THE New_Wallet SHALL integrate with OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
3. THE New_Wallet SHALL use AES-256-GCM encryption with Argon2id key derivation for keystores
4. THE New_Wallet SHALL use secrecy::Secret for secure memory handling of private keys
5. THE New_Wallet SHALL zeroize sensitive data after use
6. THE New_Wallet SHALL provide hardware wallet integration points (Ledger, Trezor)
7. THE New_Wallet SHALL never log or expose private keys in any form
8. THE New_Wallet SHALL use ONLY standard cryptographic libraries (no custom crypto)
9. THE New_Wallet SHALL document security principles and audit checklist in `security/README.md`

### Requirement 4: Multi-Chain Architecture Implementation

**User Story:** As a developer, I want to implement a modular multi-chain architecture, so that the wallet can support multiple blockchain networks extensibly.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement a ChainAdapter trait in `chains/mod.rs`
2. THE New_Wallet SHALL define chain-agnostic types in `chains/types.rs` (Balance, TxHash, ChainInfo, etc.)
3. THE New_Wallet SHALL implement an EVM adapter in `chains/evm/` using Alloy for Ethereum-compatible chains
4. THE New_Wallet SHALL support multiple EVM networks (Ethereum Mainnet, PulseChain, Polygon, Arbitrum, Optimism)
5. THE New_Wallet SHALL support custom network addition with RPC URL configuration
6. THE New_Wallet SHALL implement balance querying through the ChainAdapter trait
7. THE New_Wallet SHALL implement transaction broadcasting through the ChainAdapter trait
8. THE New_Wallet SHALL implement gas estimation through the ChainAdapter trait
9. THE New_Wallet SHALL support network switching with proper state management
10. THE New_Wallet SHALL ensure core business logic in `core/` uses ONLY the ChainAdapter trait (no chain-specific imports)
11. THE New_Wallet SHALL document how to add new chain types in `chains/README.md`

### Requirement 5: Transaction Management Implementation

**User Story:** As a developer, I want to implement comprehensive transaction management, so that users can send transactions securely and reliably.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement transaction creation and validation
2. THE New_Wallet SHALL implement transaction signing using WalletService
3. THE New_Wallet SHALL implement automatic nonce management
4. THE New_Wallet SHALL implement gas price estimation with EIP-1559 support
5. THE New_Wallet SHALL implement transaction broadcasting with confirmation tracking
6. THE New_Wallet SHALL implement transaction history retrieval and caching
7. THE New_Wallet SHALL support native token transfers and ERC-20 token transfers
8. THE New_Wallet SHALL provide transaction status monitoring

### Requirement 6: Dioxus GUI Implementation

**User Story:** As a developer, I want to implement a cross-platform GUI using Dioxus, so that users have a modern, responsive interface on desktop and mobile.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement the GUI using Dioxus with component-based architecture
2. THE New_Wallet SHALL support desktop targets (Windows, macOS, Linux) via dioxus-desktop
3. THE New_Wallet SHALL support mobile targets (Android, iOS) via dioxus-mobile
4. THE New_Wallet SHALL adapt GUI message handlers to communicate with the modular backend via use_coroutine
5. THE New_Wallet SHALL use Dioxus reactive state management (use_signal, use_context)
6. THE New_Wallet SHALL use use_future and use_coroutine for async backend operations
7. THE New_Wallet SHALL implement subscriptions for real-time updates (balance changes, transaction status)
8. THE New_Wallet SHALL implement clipboard operations (wasm-clipboard for web/mobile, native on desktop)
9. THE New_Wallet SHALL maintain responsive UI during async operations
10. THE New_Wallet SHALL organize GUI code in `gui/` module with clear separation from backend logic
11. THE New_Wallet SHALL implement the following views:
    - Dashboard: account overview, balance, quick actions
    - Send: transaction form with validation and confirmation modal
    - Receive: address display with QR code (behind qr feature)
    - History: transaction list with filtering and search
    - Settings: network, account, preferences, security management
    - Import/Export: account import/export with security warnings
12. THE New_Wallet SHALL implement reusable widget components (address display, balance display, status badge, network selector)
13. THE New_Wallet SHALL document GUI code quality improvements in `gui/IMPROVEMENTS.md`
14. THE New_Wallet SHALL document GUI adaptations from the Iced reference in `gui/CHANGES.md`
15. THE New_Wallet SHALL follow Dioxus best practices for component composition, state management, and styling

### Requirement 7: Dependency Management and Build System

**User Story:** As a developer, I want to set up a clean dependency structure and build system, so that the project is maintainable and builds efficiently.

#### Acceptance Criteria

1. THE New_Wallet SHALL use Cargo workspace for multi-crate organization
2. THE New_Wallet SHALL use Alloy for Ethereum operations (latest stable version)
3. THE New_Wallet SHALL use Dioxus for GUI framework (latest stable version)
4. THE New_Wallet SHALL use feature flags for optional functionality (hardware wallets, QR codes, audio, telemetry)
5. THE New_Wallet SHALL configure build optimization profiles for release builds
6. THE New_Wallet SHALL include platform-specific dependencies (OS keychain libraries)
7. THE New_Wallet SHALL minimize dependency bloat and avoid duplicate dependencies
8. THE New_Wallet SHALL compile without errors or warnings on stable Rust

### Requirement 8: Testing and Validation

**User Story:** As a developer, I want comprehensive test coverage, so that I can ensure the wallet functions correctly and securely.

#### Acceptance Criteria

1. THE New_Wallet SHALL include unit tests for all wallet core operations
2. THE New_Wallet SHALL include unit tests for account creation, import, and export
3. THE New_Wallet SHALL include unit tests for transaction signing and validation
4. THE New_Wallet SHALL include unit tests for keystore encryption and decryption
5. THE New_Wallet SHALL include integration tests for chain adapter operations
6. THE New_Wallet SHALL verify HD wallet derivation produces correct addresses (test vectors)
7. THE New_Wallet SHALL include property-based tests for cryptographic operations
8. THE New_Wallet SHALL achieve minimum 80% code coverage for core modules

### Requirement 9: Data Persistence and Storage

**User Story:** As a user, I want my wallet data to persist securely across sessions, so that I don't lose my accounts and settings.

#### Acceptance Criteria

1. THE New_Wallet SHALL persist encrypted keystores to disk
2. THE New_Wallet SHALL persist account metadata (names, derivation paths, types)
3. THE New_Wallet SHALL persist network configurations and custom networks
4. THE New_Wallet SHALL persist custom token configurations
5. THE New_Wallet SHALL cache transaction history locally
6. THE New_Wallet SHALL use platform-appropriate storage locations (app data directories)
7. THE New_Wallet SHALL implement atomic writes to prevent data corruption
8. THE New_Wallet SHALL validate data integrity on load

### Requirement 10: Documentation and Developer Guide

**User Story:** As a developer, I want comprehensive documentation, so that I can understand the architecture and contribute to the codebase.

#### Acceptance Criteria

1. THE New_Wallet SHALL document the overall architecture and design decisions
2. THE New_Wallet SHALL document the ChainAdapter trait and how to add new chains
3. THE New_Wallet SHALL provide code examples for common wallet operations
4. THE New_Wallet SHALL document the GUI layer and Dioxus integration patterns
5. THE New_Wallet SHALL document security considerations and best practices
6. THE New_Wallet SHALL document the build and development setup process (including mobile)
7. THE New_Wallet SHALL document testing procedures and how to run tests
8. THE New_Wallet SHALL include inline code documentation (rustdoc) for public APIs

### Requirement 11: Performance and Resource Usage

**User Story:** As a user, I want the wallet to be fast and responsive, so that I can perform operations without delays.

#### Acceptance Criteria

1. THE New_Wallet SHALL initialize within 2 seconds on modern hardware
2. THE New_Wallet SHALL sign transactions within 100ms
3. THE New_Wallet SHALL query balances within 1 second (network dependent)
4. THE New_Wallet SHALL maintain memory usage under 100MB during normal operation
5. THE New_Wallet SHALL use async operations to keep GUI responsive during backend work
6. THE New_Wallet SHALL batch network requests where possible to reduce latency
7. THE New_Wallet SHALL unlock keystores within 500ms (excluding user password entry time)

### Requirement 12: Error Handling and Recovery

**User Story:** As a developer, I want robust error handling, so that errors are properly reported and the system can recover gracefully.

#### Acceptance Criteria

1. THE New_Wallet SHALL define comprehensive error types for all operations
2. THE New_Wallet SHALL provide user-friendly error messages in the GUI
3. THE New_Wallet SHALL include error context for debugging (error chains)
4. THE New_Wallet SHALL implement graceful error recovery where possible
5. WHEN keystore operations fail, THE New_Wallet SHALL provide clear error messages
6. WHEN network operations fail, THE New_Wallet SHALL implement retry logic with exponential backoff
7. THE New_Wallet SHALL log errors with sufficient context for debugging
8. THE New_Wallet SHALL never panic in production code (use Result types)

### Requirement 13: Security Standards and Best Practices

**User Story:** As a security-conscious developer, I want to implement security best practices, so that user funds remain protected.

#### Acceptance Criteria

1. THE New_Wallet SHALL use only standard cryptographic libraries (no custom crypto)
2. THE New_Wallet SHALL use secure memory handling for private keys (secrecy::Secret)
3. THE New_Wallet SHALL never log or expose private keys in any form
4. THE New_Wallet SHALL integrate with OS keychain for secure key storage
5. THE New_Wallet SHALL validate password strength (minimum length, complexity)
6. THE New_Wallet SHALL implement rate limiting for authentication attempts
7. THE New_Wallet SHALL zeroize all sensitive data after use
8. THE New_Wallet SHALL follow OWASP security guidelines for wallet applications

### Requirement 14: Core Feature Implementation

**User Story:** As a product owner, I want to implement all essential wallet features, so that users have a complete and functional wallet.

#### Acceptance Criteria

1. THE New_Wallet SHALL support multiple account types (HD, imported private key, hardware)
2. THE New_Wallet SHALL support multiple networks (Ethereum, PulseChain, Polygon, Arbitrum, Optimism, custom)
3. THE New_Wallet SHALL support native token transfers and ERC-20 token transfers
4. THE New_Wallet SHALL support account export (private key, mnemonic) with security warnings
5. THE New_Wallet SHALL support custom token management (add, remove, edit)
6. THE New_Wallet SHALL display transaction history with status and details
7. THE New_Wallet SHALL support network switching with proper state updates
8. THE New_Wallet SHALL provide hardware wallet integration (Ledger, Trezor)

### Requirement 15: Mobile Platform Support

**User Story:** As a mobile user, I want to use the wallet on Android and iOS, so that I can manage my crypto on the go.

#### Acceptance Criteria

1. THE New_Wallet SHALL support Android via dioxus-mobile with cargo-ndk or cargo-apk
2. THE New_Wallet SHALL support iOS via dioxus-mobile with Xcode project configuration
3. THE New_Wallet SHALL configure AndroidManifest.xml with required permissions (Internet, USB for hardware wallets)
4. THE New_Wallet SHALL configure iOS entitlements for keychain and network access
5. THE New_Wallet SHALL adapt UI layouts for mobile screen sizes
6. THE New_Wallet SHALL handle mobile-specific lifecycle events (background, foreground, lock)
7. THE New_Wallet SHALL use platform-appropriate secure storage on mobile (Android Keystore, iOS Secure Enclave)

### Requirement 16: Cross-Project Learning and Improvement Documentation

**User Story:** As a developer, I want to document improvements and insights discovered during implementation, so that all three wallet implementations can benefit from this work.

#### Acceptance Criteria

1. THE New_Wallet SHALL maintain an `IMPROVEMENTS.md` document tracking discovered improvements
2. THE New_Wallet SHALL document architectural insights applicable to Vaughan-Iced and Vaughan-Tauri
3. THE New_Wallet SHALL document GUI patterns from Dioxus that work well for wallet UX
4. THE New_Wallet SHALL document any anti-patterns or issues found in reference implementations
5. THE New_Wallet SHALL categorize improvements by: Architecture, Dioxus-specific, Security, Performance
6. THE New_Wallet SHALL document the rationale for design decisions that differ from either reference
7. THE New_Wallet SHALL create actionable recommendations for backporting improvements
8. THE New_Wallet SHALL maintain a comparison matrix of architectural decisions across all three implementations
9. WHEN discovering an improvement, THE New_Wallet SHALL document: what, why, which projects benefit, effort, priority
10. THE New_Wallet SHALL review improvements quarterly for potential backporting

### Requirement 17: Minimalism and Dependency Debloating

**User Story:** As a developer, I want to build a lightweight wallet with minimal dependencies, so that the binary is small, fast, and has a reduced attack surface.

#### Acceptance Criteria

1. THE New_Wallet SHALL audit all dependencies for necessity
2. THE New_Wallet SHALL remove unused or redundant dependencies
3. THE New_Wallet SHALL prefer lightweight alternatives when functionality overlaps
4. THE New_Wallet SHALL use feature flags to make heavy dependencies optional
5. THE New_Wallet SHALL document dependency decisions in `DEPENDENCIES.md`
6. THE New_Wallet SHALL identify and remove duplicate functionality, unused features, dev-only deps
7. THE New_Wallet SHALL measure and document binary size, compilation time, memory usage, dependency count
8. THE New_Wallet SHALL set binary size targets:
   - Desktop release binary < 20MB (stripped)
   - Minimal feature set < 15MB
   - Full feature set < 25MB
9. THE New_Wallet SHALL optimize Cargo.toml with minimal feature flags and `default-features = false`
10. THE New_Wallet SHALL document bloat removed from reference implementations in `DEBLOAT.md`
11. WHEN choosing between dependencies, THE New_Wallet SHALL prefer smaller size, fewer transitive deps, better audited code
12. THE New_Wallet SHALL create comparison metrics vs Vaughan-Iced and Vaughan-Tauri

### Requirement 18: Balance Monitoring and Audio Notification System

**User Story:** As a user, I want to be notified with sound alerts when my balance changes, so that I'm immediately aware of incoming transactions.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement a balance watcher module in `monitoring/balance_watcher.rs`
2. THE New_Wallet SHALL implement token balance polling for all active accounts
3. THE New_Wallet SHALL poll balances at configurable intervals (default: 10 seconds)
4. THE New_Wallet SHALL detect balance increases and decreases
5. THE New_Wallet SHALL implement an audio notification system in `audio/` module (behind `audio` feature flag)
6. THE New_Wallet SHALL play sound alerts when balance increases, decreases, or transaction confirms
7. THE New_Wallet SHALL support multiple sound types: success, warning, notification
8. THE New_Wallet SHALL make audio notifications optional via feature flag `audio`
9. THE New_Wallet SHALL allow users to enable/disable sound alerts in settings
10. THE New_Wallet SHALL allow users to configure polling interval in settings
11. THE New_Wallet SHALL implement efficient polling with batch queries and exponential backoff on RPC errors
12. THE New_Wallet SHALL track balance history for change detection
13. THE New_Wallet SHALL integrate balance watcher with Dioxus subscriptions for real-time UI updates
14. THE New_Wallet SHALL handle network errors gracefully without disrupting the UI
15. THE New_Wallet SHALL document the monitoring system in `monitoring/README.md`

### Requirement 19: dApp Browser Integration (Tauri + Dioxus)

**User Story:** As a user, I want to interact with decentralized applications through a secure browser, so that I can use DeFi protocols while keeping my wallet keys isolated.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement a separate Tauri + Dioxus dApp browser as a child process
2. THE New_Wallet SHALL communicate with the dApp browser via IPC using the `interprocess` crate
3. THE New_Wallet SHALL use a shared `vaughan-ipc-types` crate for type-safe message protocol
4. THE New_Wallet SHALL implement IPC message types: GetAccounts, SignTransaction, RequestApproval, GetNetworkInfo, SwitchNetwork
5. THE New_Wallet SHALL inject a `window.ethereum` provider into the dApp browser WebView
6. THE New_Wallet SHALL implement EIP-1193 provider API in the injected provider
7. THE New_Wallet SHALL implement EIP-6963 wallet discovery protocol for multi-wallet support
8. THE New_Wallet SHALL spawn the Tauri dApp browser process when user clicks "Open dApp Browser"
9. THE New_Wallet SHALL authenticate the IPC connection using a shared secret token
10. THE New_Wallet SHALL validate all incoming IPC requests from the browser
11. THE New_Wallet SHALL require explicit user approval in the Dioxus UI for all sensitive operations
12. THE New_Wallet SHALL display approval modals with full transaction details before signing
13. THE New_Wallet SHALL never load or execute dApp code in the wallet process
14. THE New_Wallet SHALL isolate the dApp browser in a separate OS process (Tauri) with its own memory space
15. THE New_Wallet SHALL bundle the Tauri dApp browser executable with the wallet in a single package
16. THE New_Wallet SHALL implement a browser UI in Dioxus inside Tauri with address bar, navigation, and connection status
17. THE New_Wallet SHALL apply the same theme and branding to both wallet and browser windows
18. THE New_Wallet SHALL implement platform-specific window grouping:
    - Windows: Same AppUserModelID for taskbar grouping
    - macOS: Browser as helper process (no Dock icon, LSUIElement=true)
    - Linux: Same WM_CLASS for window manager grouping
19. THE New_Wallet SHALL handle browser process lifecycle (spawn, monitor, terminate)
20. THE New_Wallet SHALL implement graceful error handling for IPC failures
21. THE New_Wallet SHALL log all dApp interactions for security auditing (no sensitive data)
22. THE New_Wallet SHALL emit all required EIP-1193 events (connect, disconnect, accountsChanged, chainChanged)
23. THE New_Wallet SHALL support all standard JSON-RPC methods required by EIP-1193
24. THE New_Wallet SHALL announce wallet metadata via EIP-6963 (name, icon, UUID, description)
25. THE New_Wallet SHALL document the multi-process security architecture for transparency and trust

### Requirement 20: Build, Packaging, and Release Process

**User Story:** As a project maintainer, I want proper build and deployment configuration, so that I can release the wallet to users reliably across all platforms as a single package.

#### Acceptance Criteria

1. THE New_Wallet SHALL use cargo-bundle to create platform-specific installers
2. THE New_Wallet SHALL package both executables (Dioxus wallet + Tauri browser) in a single installer
3. THE New_Wallet SHALL provide release build configuration with optimizations (opt-level="z", LTO, strip)
4. THE New_Wallet SHALL support cross-platform builds (Windows, macOS, Linux, Android, iOS)
5. THE New_Wallet SHALL create platform-specific installers: Windows .msi/.exe, macOS .app/.dmg, Linux .deb/.rpm/.AppImage
6. THE New_Wallet SHALL include CI/CD configuration for automated testing (GitHub Actions)
7. THE New_Wallet SHALL include code signing configuration for release builds
8. THE New_Wallet SHALL configure [package.metadata.bundle] with name, identifier, icon, category, descriptions
9. THE New_Wallet SHALL define both executables as [[bin]] entries in Cargo.toml
10. THE New_Wallet SHALL implement auto-updater using self_update crate
11. THE New_Wallet SHALL include version management and CHANGELOG.md
12. THE New_Wallet SHALL document the release process and deployment checklist
13. THE New_Wallet SHALL configure feature flags with binary size impact documentation
14. THE New_Wallet SHALL measure and document binary sizes for different feature configurations
15. THE New_Wallet SHALL ensure users download one file, run one installer, and see one application icon

### Requirement 21: Logging, Monitoring, and Telemetry

**User Story:** As a developer and maintainer, I want comprehensive logging and optional telemetry, so that I can debug issues and understand usage patterns.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement structured logging using tracing crate
2. THE New_Wallet SHALL configure log levels (ERROR, WARN, INFO, DEBUG, TRACE)
3. THE New_Wallet SHALL write logs to daily rotating files in platform-appropriate locations
4. THE New_Wallet SHALL NEVER log sensitive data (private keys, mnemonics, passwords)
5. THE New_Wallet SHALL implement opt-in telemetry (behind telemetry feature flag)
6. THE New_Wallet SHALL show consent dialog on first run for telemetry
7. THE New_Wallet SHALL collect only anonymous usage data when telemetry is enabled
8. THE New_Wallet SHALL NOT collect private keys, mnemonics, wallet addresses, or transaction details
9. THE New_Wallet SHALL allow users to disable telemetry at any time
10. THE New_Wallet SHALL encrypt all telemetry data in transit (HTTPS)
11. THE New_Wallet SHALL implement Sentry integration for error reporting (optional, with data scrubbing)

### Requirement 22: Hardware Wallet Integration Details

**User Story:** As a user, I want to use hardware wallets (Ledger, Trezor) for maximum security, so that my private keys never leave the hardware device.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement hardware wallet support behind hardware-wallets feature flag
2. THE New_Wallet SHALL support Ledger devices using ledger-apdu crate
3. THE New_Wallet SHALL support Trezor devices using trezor-client crate
4. THE New_Wallet SHALL implement device connection and discovery
5. THE New_Wallet SHALL implement address derivation from hardware wallets
6. THE New_Wallet SHALL implement transaction signing via hardware wallets
7. THE New_Wallet SHALL handle device disconnection and reconnection gracefully
8. THE New_Wallet SHALL implement retry logic for device communication failures
9. THE New_Wallet SHALL show device confirmation UI in Dioxus wallet
10. THE New_Wallet SHALL route signing requests based on account type (software vs hardware)
11. THE New_Wallet SHALL display "waiting for device confirmation" status
12. THE New_Wallet SHALL timeout hardware wallet operations after reasonable duration

### Requirement 23: Performance Optimization Strategies

**User Story:** As a user, I want the wallet to be fast and efficient, so that operations complete quickly and resource usage is minimal.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement RPC response caching using moka crate (balance 10s TTL, gas price 15s TTL, nonce 5s TTL)
2. THE New_Wallet SHALL implement cache invalidation on relevant operations
3. THE New_Wallet SHALL implement HTTP connection pooling for RPC requests
4. THE New_Wallet SHALL configure connection pool with appropriate size and timeout
5. THE New_Wallet SHALL offload CPU-intensive operations to blocking thread pool (encryption, key derivation, password hashing)
6. THE New_Wallet SHALL use tokio::task::spawn_blocking for blocking operations
7. THE New_Wallet SHALL batch network requests where possible
8. THE New_Wallet SHALL implement efficient balance polling with batch queries
9. THE New_Wallet SHALL use exponential backoff for RPC errors
10. THE New_Wallet SHALL maintain responsive GUI during all backend operations

### Requirement 24: Localization (Internationalization)

**User Story:** As a user, I want the wallet interface in my preferred language, so that I can use the wallet comfortably.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement localization using rust-i18n crate (optional feature)
2. THE New_Wallet SHALL externalize all user-facing strings to translation files
3. THE New_Wallet SHALL support English (en) as the default language
4. THE New_Wallet SHALL detect system locale automatically
5. THE New_Wallet SHALL allow users to change language in settings
6. THE New_Wallet SHALL organize translations in locales/ directory
7. THE New_Wallet SHALL use translation keys with t! macro throughout the codebase
8. THE New_Wallet SHALL persist language preference across sessions

### Requirement 25: Security Audit and Dependency Vetting

**User Story:** As a security-conscious developer, I want to vet all dependencies and conduct security audits, so that the wallet is secure and trustworthy.

#### Acceptance Criteria

1. THE New_Wallet SHALL use cargo-vet for dependency vetting
2. THE New_Wallet SHALL use cargo-audit for vulnerability scanning
3. THE New_Wallet SHALL run security audits in CI/CD pipeline
4. THE New_Wallet SHALL document dependency vetting process
5. THE New_Wallet SHALL maintain internal security checklist covering crypto, key handling, input validation, rate limiting, code signing
6. THE New_Wallet SHALL conduct external security audit before major releases
7. THE New_Wallet SHALL implement responsible disclosure policy
8. THE New_Wallet SHALL provide security contact (email, PGP key)
9. THE New_Wallet SHALL define response timelines: Critical 7d, High 14d, Medium 30d, Low 90d
10. THE New_Wallet SHALL follow coordinated disclosure process

### Requirement 26: Platform-Specific Integration Details

**User Story:** As a developer, I want platform-specific implementations documented, so that the wallet works correctly on Windows, macOS, Linux, Android, and iOS.

#### Acceptance Criteria

1. THE New_Wallet SHALL implement Windows-specific features (Credential Manager, %APPDATA% paths, signtool signing)
2. THE New_Wallet SHALL implement macOS-specific features (Keychain, ~/Library/ paths, notarization, universal binary)
3. THE New_Wallet SHALL implement Linux-specific features (Secret Service API, XDG paths, GNOME/KWallet support, AppImage)
4. THE New_Wallet SHALL implement Android-specific features (Android Keystore, app data paths, cargo-ndk build)
5. THE New_Wallet SHALL implement iOS-specific features (Secure Enclave, ~/Library/ paths, Xcode entitlements)
6. THE New_Wallet SHALL test on major platforms: Windows 10/11, macOS 11+, Ubuntu/Fedora/Arch, Android 10+, iOS 14+
7. THE New_Wallet SHALL handle platform-specific permission dialogs
8. THE New_Wallet SHALL document platform-specific considerations

### Requirement 27: Development Environment and Contributing

**User Story:** As a contributor, I want clear development setup instructions and contribution guidelines, so that I can contribute effectively.

#### Acceptance Criteria

1. THE New_Wallet SHALL document required Rust version and Dioxus CLI setup
2. THE New_Wallet SHALL document recommended editor setup
3. THE New_Wallet SHALL document how to run tests and benchmarks
4. THE New_Wallet SHALL provide contribution guidelines (rustfmt, clippy, commit format, PR checklist)
5. THE New_Wallet SHALL document development workflow including mobile simulator setup
6. THE New_Wallet SHALL define code quality standards (no clippy warnings, rustfmt, docs, tests)
7. THE New_Wallet SHALL document testing commands including mobile targets

# Vaughan Security Checklist

This checklist tracks the current security hardening implemented in the repo.

## Implemented

- [x] Password validation requires minimum length and mixed character classes.
- [x] Password hashing uses Argon2id.
- [x] Secret storage uses AES-256-GCM and the OS keychain.
- [x] Mnemonic validation is enforced before storage or export.
- [x] Sensitive operations return structured `WalletError` values.
- [x] Authentication attempts are rate limited with lockout after repeated failures.
- [x] Network operations surface user-friendly retryable errors.
- [x] Transaction and balance flows avoid blocking the UI thread.
- [x] Core security helpers have unit and property test coverage.

## Remaining Review Items

- [ ] Confirm external security contact details.
- [ ] Run `cargo audit` before release.
- [ ] Perform an end-to-end manual wallet recovery review.
- [ ] Review dApp permission prompts and origin checks.
- [ ] Verify release signing and distribution process.


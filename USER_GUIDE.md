# Vaughan User Guide (Desktop Wallet)

> **Prototype status:** Vaughan is currently a prototype under active development. Expect rough edges, incomplete features, and breaking changes between revisions.

## 1. Installation & Startup

1. **Download / build** the Vaughan desktop wallet (for now: build from source).
2. From a terminal in the project root, run:

   ```bash
   cargo run -p vaughan-dioxus
   ```

3. The Vaughan wallet window will open on the **Dashboard** screen.

> Note: This build is for **development and testing only**. Use testnets, not real funds.

## 2. First-Time Setup

### Create a new wallet (HD)

1. Go to **Import/Export**.
2. Choose **Create New Wallet** (or equivalent option once wired).
3. Write down your **seed phrase** on paper and store it safely.
4. Set a **strong password** (minimum 12 chars, mix of letters, numbers, symbols).

### Import an existing wallet

You will be able to:

- Import a **mnemonic** (seed phrase), or
- Import a **private key** for a single account.

Use the **Import/Export** view and follow on-screen instructions.

## 3. Basic Usage

### Viewing your balance

- The **Dashboard** shows:
  - Active account address (shortened).
  - Native asset balance for the active network.
  - Recent balance changes (when monitoring is enabled).

### Sending funds

1. Navigate to **Send**.
2. Enter:
   - **Recipient address** (0x… for EVM).
   - **Amount** to send.
3. The wallet estimates gas/fees (when connected).
4. Review the summary and confirm.
5. After sending, you can track status in **History**.

### Receiving funds

1. Navigate to **Receive**.
2. Copy your address using the **Copy** button.
3. Optionally show a **QR code** if the QR feature is enabled.

Share this address with the sender; do not share your seed phrase or private keys.

### Transaction history

- Go to **History** to see:
  - Recent native and token transfers.
  - Status badges (Pending / Confirmed / Failed).
  - Basic search/filter options (as implemented).

The wallet periodically refreshes pending transactions until they are confirmed or failed.

## 4. Networks & Tokens

### Switching networks

1. Open **Settings**.
2. Use the **Network** section to:
   - View built-in networks (e.g., Ethereum mainnet, testnets).
   - Set the active network.

### Adding a custom network

In **Settings → Networks**, you can:

1. Provide:
   - Network name.
   - RPC URL (HTTPS endpoint).
   - Chain ID.
   - Optional explorer URLs.
2. Save to add the custom network, then select it as active.

### Managing tokens

In **Settings → Tokens**, you can:

- **List** tracked ERC‑20 tokens for the current chain.
- **Add** a token using its contract address, symbol, name, and decimals.
- **Remove** a token to stop tracking it.

Token balances then appear in the Dashboard and History (as implemented).

## 5. Security Best Practices

- **Seed phrase**:
  - Write it down on paper.
  - Store it offline in a safe place.
  - Never type it into websites or share it with anyone.

- **Password**:
  - Use a unique, strong password (12+ chars, mixed types).
  - Consider a password manager.

- **Devices**:
  - Keep your OS and antivirus up to date.
  - Only run Vaughan builds you trust (from source you control or official releases).

- **dApps (future Tauri browser)**:
  - Only connect to dApps you trust.
  - Check permissions and transaction details before approving.

## 6. Troubleshooting

- **Wallet won’t unlock**:
  - Check that Caps Lock isn’t on.
  - Remember that passwords are case-sensitive and must meet strength requirements.

- **Balance not updating**:
  - Ensure you’re on the correct network.
  - Check your internet connection.
  - Try restarting the wallet.

- **Transactions stuck as pending**:
  - Network congestion or low gas can delay confirmation.
  - Verify the transaction status using a block explorer for the active network.

If you encounter errors, you can share error messages (but never secrets) with a developer or support channel for further help.


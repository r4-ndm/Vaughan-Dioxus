// Injected into every frame (shell, allowlisted external top-level, cross-origin iframes)
// via Tauri `initialization_script_for_all_frames`, so dApps see window.ethereum.
//
// Bridge selection (topnav-3):
// - Use direct `__TAURI__.core.invoke` / `invoke` in this frame when available (shell or WebviewUrl::External top).
// - If this frame has no invoke and is nested, use postMessage to `window.top` (shell index.html listener or the
//   relay below on allowlisted https / loopback top-level pages).
// - Allowlisted top-level https / loopback installs a VAUGHAN_IPC relay (parity with index.html) for nested frames.
//
// Wait until invoke is ready before injecting — early return caused silent no-provider bugs.
(function () {
  if (window.__VAUGHAN_ETH_INJECTED__) return;

  /** Keep in sync with `vaughan-trusted-hosts` ALLOWED_HTTPS_HOST_SUFFIXES and `index.html` trustedWalletBridgeOrigin. */
  var TRUSTED_HOST_SUFFIXES = [
    "uniswap.org",
    "uniswap.com",
    "sushi.com",
    "pancakeswap.finance",
    "curve.fi",
    "aave.com",
    "compound.finance",
    "1inch.com",
    "opensea.io",
    "stargate.finance",
    "v4.testnet.pulsechain.com",
    "pulsex.com",
    "pulsex.mypinata.cloud",
    "piteas.io",
    "gopulse.com",
    "internetmoney.io",
    "provex.com",
    "libertyswap.finance",
    "0xcurv.win",
    "pump.tires",
    "9mm.pro",
    "9inch.io",
    "hyperliquid.xyz",
    "asterdex.com",
  ];

  function isTrustedWalletPageOrigin(href) {
    try {
      var u = new URL(href);
      var h = u.hostname.toLowerCase().replace(/\.$/, "");
      if (u.protocol === "http:") {
        return h === "localhost" || h === "127.0.0.1";
      }
      if (u.protocol !== "https:") return false;
      for (var i = 0; i < TRUSTED_HOST_SUFFIXES.length; i++) {
        var s = TRUSTED_HOST_SUFFIXES[i];
        if (h === s || h.endsWith("." + s)) return true;
      }
      return false;
    } catch (e) {
      return false;
    }
  }

  /**
   * Shell document, the dApp iframe (direct child of shell), or a nested iframe on a
   * trusted host. Cross-origin *child* frames on untrusted hosts are excluded by the
   * hostname check above (ads, trackers). When parent is another trusted origin we
   * cannot read (e.g. www.* embedding app.*), still inject — otherwise many dApps get
   * no provider while Uniswap (single document under shell) works fine.
   */
  function shouldInstallBridgeProvider() {
    if (!needsTopWindowBridge()) return true;
    if (!isTrustedWalletPageOrigin(window.location.href)) return false;
    try {
      if (window.parent === window.top) return true;
      return isTrustedWalletPageOrigin(window.parent.location.href);
    } catch (e) {
      return true;
    }
  }

  function getInvoke() {
    if (
      window.__TAURI__ &&
      window.__TAURI__.core &&
      typeof window.__TAURI__.core.invoke === "function"
    ) {
      return window.__TAURI__.core.invoke;
    }
    if (window.__TAURI__ && typeof window.__TAURI__.invoke === "function") {
      return window.__TAURI__.invoke;
    }
    return null;
  }

  function waitForTauri(setup, attempts) {
    if (attempts === undefined) attempts = 40;
    if (getInvoke()) {
      setTimeout(setup, 0);
    } else if (attempts > 0) {
      setTimeout(function () {
        waitForTauri(setup, attempts - 1);
      }, 100);
    } else {
      console.error(
        "[Vaughan] Tauri invoke not available in this frame (expected only for top-level shell)"
      );
    }
  }

  /**
   * True when this frame has no in-process invoke and must use postMessage to window.top
   * (iframe under shell or under an allowlisted top document that runs the IPC relay).
   */
  function needsTopWindowBridge() {
    if (getInvoke()) return false;
    try {
      return window.top !== window.self;
    } catch (e) {
      return true;
    }
  }

  // Cross-origin nested frames must not install the bridge: the shell rejects their origin,
  // which previously left IPC pending until a 120s timeout and slowed the whole tab.
  if (!shouldInstallBridgeProvider()) return;

  function setup() {
    if (window.__VAUGHAN_ETH_INJECTED__) return;
    var invFn = getInvoke();
    var useParentBridge = needsTopWindowBridge();
    if (!useParentBridge && !invFn) return;

    window.__VAUGHAN_ETH_INJECTED__ = true;

    (function installTopDocumentIpcRelayIfNeeded() {
      if (window.__VAUGHAN_TOP_IPC_RELAY__) return;
      if (window !== window.top) return;
      var relayTop = false;
      try {
        var proto = window.location.protocol;
        if (proto === "https:" && isTrustedWalletPageOrigin(window.location.href)) {
          relayTop = true;
        } else if (proto === "http:") {
          var hn = (window.location.hostname || "").toLowerCase();
          if (hn === "localhost" || hn === "127.0.0.1") relayTop = true;
        }
      } catch (e) {
        return;
      }
      if (!relayTop) return;
      window.__VAUGHAN_TOP_IPC_RELAY__ = true;
      window.addEventListener("message", function (e) {
        if (!e.data || e.data.type !== "VAUGHAN_IPC") return;
        if (!isTrustedWalletPageOrigin(e.origin + "/")) {
          if (
            e.data.cmd === "ipc_request" &&
            e.source &&
            e.source.postMessage &&
            e.data.id != null
          ) {
            e.source.postMessage(
              {
                type: "VAUGHAN_IPC_RESPONSE",
                id: e.data.id,
                error: {
                  message: "Origin not allowlisted for wallet bridge",
                  code: 4901,
                },
              },
              e.origin
            );
          }
          return;
        }
        if (e.data.cmd !== "ipc_request") return;
        var mid = e.data.id;
        var margs = e.data.args;
        var inv = getInvoke();
        if (!e.source || !e.source.postMessage) return;
        if (!inv) {
          e.source.postMessage(
            {
              type: "VAUGHAN_IPC_RESPONSE",
              id: mid,
              error: {
                message: "Tauri invoke not ready in top frame",
                code: 4900,
              },
            },
            e.origin
          );
          return;
        }
        inv("ipc_request", margs)
          .then(function (result) {
            e.source.postMessage(
              { type: "VAUGHAN_IPC_RESPONSE", id: mid, result: result },
              e.origin
            );
          })
          .catch(function (err) {
            e.source.postMessage(
              {
                type: "VAUGHAN_IPC_RESPONSE",
                id: mid,
                error: {
                  message: String(err && err.message ? err.message : err),
                  code: err && err.code,
                },
              },
              e.origin
            );
          });
      });
    })();

  function makeEthRpcError(code, message, data) {
    var err = new Error(message);
    err.code = code;
    if (data !== undefined) err.data = data;
    return err;
  }

  function normalizeProviderError(e) {
    if (e && typeof e.code === "number") return e;
    return makeEthRpcError(-32603, (e && e.message) || "Internal error");
  }

  function hexQtyToDecString(value) {
    if (value == null) return null;
    var s = String(value).trim();
    if (!s) return null;
    if (s.startsWith("0x") || s.startsWith("0X")) return BigInt(s).toString();
    return BigInt(s).toString();
  }

  function mapIpcError(payload, fallbackCode, fallbackMsg) {
    var code = Number(payload && payload.code);
    var msg = (payload && payload.message) || fallbackMsg;
    if (
      [4001, 4100, 4200, 4900, -32600, -32601, -32602, -32603].indexOf(code) !==
      -1
    ) {
      return makeEthRpcError(code, msg);
    }
    return makeEthRpcError(fallbackCode, msg);
  }

  function tauriInvoke(cmd, args) {
    if (useParentBridge) {
      if (cmd !== "ipc_request") {
        return Promise.reject(
          makeEthRpcError(-32601, "Unsupported bridge command: " + cmd)
        );
      }
      return new Promise(function (resolve, reject) {
        var id =
          "v_" +
          Math.random().toString(36).slice(2) +
          "_" +
          Date.now().toString(36);
        var timeout = setTimeout(function () {
          window.removeEventListener("message", onReply);
          reject(makeEthRpcError(4900, "Wallet bridge timeout"));
        }, 120000);
        function onReply(event) {
          var d = event.data;
          if (!d || d.type !== "VAUGHAN_IPC_RESPONSE" || d.id !== id) return;
          clearTimeout(timeout);
          window.removeEventListener("message", onReply);
          if (d.error) {
            var er = new Error(
              (d.error && d.error.message) || String(d.error || "Bridge error")
            );
            if (d.error && typeof d.error.code === "number") {
              er.code = d.error.code;
            }
            reject(er);
          } else {
            resolve(d.result);
          }
        }
        window.addEventListener("message", onReply);
        try {
          var bridgeWin = window.top;
          if (!bridgeWin || bridgeWin === window) {
            clearTimeout(timeout);
            window.removeEventListener("message", onReply);
            reject(
              makeEthRpcError(4900, "Wallet bridge target unavailable in this frame")
            );
            return;
          }
          bridgeWin.postMessage(
            { type: "VAUGHAN_IPC", id: id, cmd: cmd, args: args },
            "*"
          );
        } catch (err) {
          clearTimeout(timeout);
          window.removeEventListener("message", onReply);
          reject(err);
        }
      });
    }
    return invFn(cmd, args);
  }

  function createEmitter() {
    var listeners = new Map();
    return {
      on: function (event, handler) {
        var arr = listeners.get(event) || [];
        arr.push(handler);
        listeners.set(event, arr);
      },
      removeListener: function (event, handler) {
        var arr = listeners.get(event) || [];
        listeners.set(
          event,
          arr.filter(function (h) {
            return h !== handler;
          })
        );
      },
      emit: function (event, payload) {
        var arr = listeners.get(event) || [];
        for (var i = 0; i < arr.length; i++) {
          try {
            arr[i](payload);
          } catch (e) {
            console.error(e);
          }
        }
      },
    };
  }

  var emitter = createEmitter();
  var currentAccounts = [];
  var currentChainIdHex = null;
  var firstConnectEmitted = false;
  /** Until true, eth_accounts returns [] so dApp "disconnect" sticks (wagmi/Uniswap poll eth_accounts). */
  var sessionAuthorized = false;
  var disconnectHoldUntilMs = 0;
  var lastRevokeAtMs = 0;
  var inflightGetAccounts = null;
  var inflightGetNetworkInfo = null;
  var lastAccountsAtMs = 0;
  var lastNetworkAtMs = 0;
  var lastAccountsValue = null;
  var lastNetworkValue = null;
  var IPC_CACHE_MS = 250;
  var DISCONNECT_HOLD_MS = 2000;

  function clearDappSession(reason) {
    sessionAuthorized = false;
    disconnectHoldUntilMs = Date.now() + DISCONNECT_HOLD_MS;
    inflightGetAccounts = null;
    inflightGetNetworkInfo = null;
    currentAccounts = [];
    firstConnectEmitted = false;
    emitter.emit("accountsChanged", []);
    emitter.emit(
      "disconnect",
      reason || { code: 4900, message: "Disconnected from dApp" }
    );
    syncLegacy();
  }

  function sessionIsDisconnected() {
    return (
      !sessionAuthorized &&
      (!currentAccounts || currentAccounts.length === 0) &&
      !firstConnectEmitted
    );
  }

  function emitDisconnect(err) {
    currentAccounts = [];
    currentChainIdHex = null;
    firstConnectEmitted = false;
    sessionAuthorized = false;
    disconnectHoldUntilMs = Date.now() + DISCONNECT_HOLD_MS;
    inflightGetAccounts = null;
    inflightGetNetworkInfo = null;
    emitter.emit(
      "disconnect",
      err || { code: 4100, message: "Wallet disconnected" }
    );
    syncLegacy();
  }

  function syncLegacy() {
    try {
      if (!ethereumProvider) return;
      ethereumProvider.chainId = currentChainIdHex;
      if (currentChainIdHex && currentChainIdHex.length > 2) {
        ethereumProvider.networkVersion = String(
          parseInt(currentChainIdHex.slice(2), 16)
        );
      } else {
        ethereumProvider.networkVersion = null;
      }
      ethereumProvider.selectedAddress =
        currentAccounts && currentAccounts.length
          ? currentAccounts[0]
          : null;
    } catch (e) {}
  }

  async function fetchNetworkInfo() {
    var now = Date.now();
    if (lastNetworkValue && now - lastNetworkAtMs <= IPC_CACHE_MS) {
      return lastNetworkValue;
    }
    if (!inflightGetNetworkInfo) {
      inflightGetNetworkInfo = tauriInvoke("ipc_request", {
        request: { type: "GetNetworkInfo" },
      })
        .then(function (resp) {
          if (resp && resp.type === "NetworkInfo") {
            lastNetworkValue = resp.payload;
            lastNetworkAtMs = Date.now();
            return resp.payload;
          }
          throw makeEthRpcError(4100, "Wallet not connected");
        })
        .finally(function () {
          inflightGetNetworkInfo = null;
        });
    }
    return inflightGetNetworkInfo;
  }

  async function fetchAccounts() {
    var now = Date.now();
    if (lastAccountsValue && now - lastAccountsAtMs <= IPC_CACHE_MS) {
      return lastAccountsValue;
    }
    if (!inflightGetAccounts) {
      inflightGetAccounts = tauriInvoke("ipc_request", {
        request: { type: "GetAccounts" },
      })
        .then(function (resp) {
          if (resp && resp.type === "Accounts") {
            lastAccountsValue = resp.payload.map(function (a) {
              return a.address;
            });
            lastAccountsAtMs = Date.now();
            return lastAccountsValue;
          }
          throw makeEthRpcError(4100, "Wallet not connected");
        })
        .finally(function () {
          inflightGetAccounts = null;
        });
    }
    return inflightGetAccounts;
  }

  function toHexChainId(dec) {
    try {
      var n = typeof dec === "string" ? BigInt(dec) : BigInt(dec);
      return "0x" + n.toString(16);
    } catch (e) {
      return "0x0";
    }
  }

  // Icon for EIP-6963 (required by many modals). This WebView has no browser extension;
  // we intentionally present as MetaMask-compatible so Uniswap/wagmi "MetaMask" path works.
  var WALLET_ICON =
    "data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIHZpZXdCb3g9IjAgMCAzMiAzMiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cmVjdCB3aWR0aD0iMzIiIGhlaWdodD0iMzIiIHJ4PSI4IiBmaWxsPSIjNGY0NmU1Ii8+PHBhdGggZD0iTTE2IDhsOCA4LTggOC04LTh6IiBmaWxsPSIjZmZmIi8+PC9zdmc+";

  var ethereumProvider = {
    isMetaMask: true,
    isVaughan: true,
    chainId: null,
    networkVersion: null,
    selectedAddress: null,
    _metamask: { isUnlocked: true },
    enable: function () {
      return this.request({ method: "eth_requestAccounts" });
    },
    isConnected: function () {
      return !!(
        sessionAuthorized &&
        currentAccounts &&
        currentAccounts.length
      );
    },
    sendAsync: function (payload, callback) {
      if (!callback) return;
      var id = payload && payload.id;
      this.request({
        method: payload.method,
        params: payload.params || [],
      })
        .then(function (result) {
          callback(null, {
            jsonrpc: "2.0",
            id: id,
            result: result,
          });
        })
        .catch(function (err) {
          callback(
            {
              message: (err && err.message) || "Error",
              code: err && err.code,
            },
            null
          );
        });
    },
    send: function (methodOrPayload, paramsOrCallback) {
      if (typeof methodOrPayload === "string") {
        return this.request({
          method: methodOrPayload,
          params: paramsOrCallback,
        });
      }
      if (typeof paramsOrCallback === "function") {
        return this.sendAsync(methodOrPayload, paramsOrCallback);
      }
      return this.request(methodOrPayload);
    },
    request: async function (req) {
      req = req || {};
      var method = req.method;
      var params = req.params;
      var m = (method || "").toLowerCase();
      try {
        if (m === "wallet_revokepermissions") {
          var revokeNow = Date.now();
          if (sessionIsDisconnected() && revokeNow - lastRevokeAtMs < 1500) {
            return null;
          }
          lastRevokeAtMs = revokeNow;
          clearDappSession({ code: 4900, message: "Permissions revoked" });
          return null;
        }

        // Legacy name used by some tooling; treat like revoke.
        if (m === "metamask_disconnect") {
          if (sessionIsDisconnected()) {
            return null;
          }
          clearDappSession({ code: 4900, message: "Disconnected" });
          return null;
        }

        if (m === "wallet_requestpermissions") {
          if (Date.now() < disconnectHoldUntilMs) {
            throw makeEthRpcError(
              4001,
              "Wallet recently disconnected for this site. Try connect again."
            );
          }
          var reqPerms = (params && params[0]) || {};
          if (!reqPerms.eth_accounts) {
            throw makeEthRpcError(4200, "Unsupported permission request");
          }
          if (
            sessionAuthorized &&
            currentAccounts &&
            currentAccounts.length
          ) {
            syncLegacy();
            return { eth_accounts: currentAccounts };
          }
          if (!currentChainIdHex) {
            var netRp = await fetchNetworkInfo();
            currentChainIdHex = toHexChainId(netRp.chain_id);
          }
          var accRp = await fetchAccounts();
          if (!accRp || !accRp.length) {
            throw makeEthRpcError(
              4100,
              "No accounts in wallet. Create or import an account in Vaughan first."
            );
          }
          sessionAuthorized = true;
          currentAccounts = accRp;
          emitter.emit("accountsChanged", accRp);
          if (!firstConnectEmitted) {
            firstConnectEmitted = true;
            emitter.emit("connect", { chainId: currentChainIdHex });
          }
          syncLegacy();
          return { eth_accounts: accRp };
        }

        if (m === "wallet_getpermissions") {
          if (!sessionAuthorized || !currentAccounts.length) return [];
          return [{ parentCapability: "eth_accounts" }];
        }

        if (m === "eth_accounts") {
          if (!sessionAuthorized) {
            syncLegacy();
            return [];
          }
          var accRo;
          if (!currentChainIdHex) {
            var pairRo = await Promise.all([
              fetchNetworkInfo(),
              fetchAccounts(),
            ]);
            currentChainIdHex = toHexChainId(pairRo[0].chain_id);
            accRo = pairRo[1];
          } else {
            accRo = await fetchAccounts();
          }
          var prevJson = JSON.stringify(currentAccounts || []);
          var nextJson = JSON.stringify(accRo || []);
          currentAccounts = accRo;
          if (prevJson !== nextJson) {
            emitter.emit("accountsChanged", accRo);
          }
          syncLegacy();
          return accRo;
        }

        if (m === "eth_requestaccounts") {
          if (Date.now() < disconnectHoldUntilMs) {
            throw makeEthRpcError(
              4001,
              "Wallet recently disconnected for this site. Try connect again."
            );
          }
          if (
            sessionAuthorized &&
            currentAccounts &&
            currentAccounts.length
          ) {
            syncLegacy();
            return currentAccounts;
          }
          var accounts;
          if (!currentChainIdHex) {
            var pair = await Promise.all([fetchNetworkInfo(), fetchAccounts()]);
            currentChainIdHex = toHexChainId(pair[0].chain_id);
            accounts = pair[1];
          } else {
            accounts = await fetchAccounts();
          }
          if (!accounts || !accounts.length) {
            throw makeEthRpcError(
              4100,
              "No accounts in wallet. Create or import an account in Vaughan first."
            );
          }
          sessionAuthorized = true;
          currentAccounts = accounts;
          emitter.emit("accountsChanged", accounts);
          if (!firstConnectEmitted) {
            firstConnectEmitted = true;
            emitter.emit("connect", { chainId: currentChainIdHex });
          }
          syncLegacy();
          return accounts;
        }

        if (m === "wallet_getcapabilities") {
          if (!currentChainIdHex) {
            var netCap = await fetchNetworkInfo();
            currentChainIdHex = toHexChainId(netCap.chain_id);
          }
          var caps = {};
          caps[currentChainIdHex] = {};
          return caps;
        }

        if (m === "eth_chainid") {
          var net1 = await fetchNetworkInfo();
          currentChainIdHex = toHexChainId(net1.chain_id);
          emitter.emit("chainChanged", currentChainIdHex);
          syncLegacy();
          return currentChainIdHex;
        }

        if (m === "eth_sendtransaction") {
          if (!sessionAuthorized) {
            throw makeEthRpcError(4100, "Wallet not connected to this site");
          }
          var tx = (params && params[0]) || {};
          var value = tx.value != null ? tx.value : "0x0";
          var from = (tx.from != null ? tx.from : currentAccounts[0]) || null;
          var to = tx.to != null ? tx.to : null;
          var chainId = tx.chainId != null ? tx.chainId : null;
          if (!to) throw makeEthRpcError(-32602, "Missing transaction 'to'");
          if (!from) throw makeEthRpcError(-32602, "Missing transaction 'from'");
          var data = tx.data != null ? tx.data : "0x";
          if (typeof data === "string" && data !== "0x" && data !== "0x0") {
            throw makeEthRpcError(4200, "Only simple transfers are supported for now");
          }
          var net2;
          if (chainId) {
            if (typeof chainId === "string" && chainId.startsWith("0x")) {
              net2 = { chain_id: Number(BigInt(chainId).toString()) };
            } else {
              net2 = { chain_id: Number(chainId) };
            }
          } else {
            net2 = await fetchNetworkInfo();
          }
          var chain_id = net2.chain_id;
          var valueDec = hexQtyToDecString(value) || "0";
          var nonceDec = hexQtyToDecString(tx.nonce);
          var gasLimitDec = hexQtyToDecString(tx.gas != null ? tx.gas : tx.gasLimit);
          var gasPriceDec = hexQtyToDecString(tx.gasPrice);
          var maxFeePerGasDec = hexQtyToDecString(tx.maxFeePerGas);
          var maxPriorityFeePerGasDec = hexQtyToDecString(
            tx.maxPriorityFeePerGas
          );
          var txData = typeof tx.data === "string" ? tx.data : null;

          var respTx = await tauriInvoke("ipc_request", {
            request: {
              type: "SignTransaction",
              payload: {
                from: from,
                to: to,
                value: valueDec,
                data: txData,
                nonce: nonceDec,
                gas_limit: gasLimitDec,
                gas_price: gasPriceDec,
                max_fee_per_gas: maxFeePerGasDec,
                max_priority_fee_per_gas: maxPriorityFeePerGasDec,
                chain_id: chain_id,
              },
            },
          });

          if (respTx && respTx.type === "SignedTransaction")
            return respTx.payload;
          if (respTx && respTx.type === "Error") {
            throw mapIpcError(respTx.payload, 4100, "Signing failed");
          }
          throw makeEthRpcError(-32603, "Unexpected response from wallet");
        }

        if (m === "personal_sign") {
          if (!sessionAuthorized) {
            throw makeEthRpcError(4100, "Wallet not connected to this site");
          }
          var message = params && params[0];
          var address = params && params[1];
          if (!address)
            throw makeEthRpcError(-32602, "Missing address for personal_sign");
          if (message == null)
            throw makeEthRpcError(-32602, "Missing message for personal_sign");
          var net3 = await fetchNetworkInfo();
          var respPs = await tauriInvoke("ipc_request", {
            request: {
              type: "SignMessage",
              payload: {
                address: address,
                message: String(message),
                chain_id: net3.chain_id,
              },
            },
          });
          if (respPs && respPs.type === "SignedMessage") return respPs.payload;
          if (respPs && respPs.type === "Error") {
            throw mapIpcError(respPs.payload, 4100, "Signing failed");
          }
          throw makeEthRpcError(-32603, "Unexpected response from wallet");
        }

        if (m === "eth_signtypeddata_v4") {
          if (!sessionAuthorized) {
            throw makeEthRpcError(4100, "Wallet not connected to this site");
          }
          var addrTd = params && params[0];
          var typedData = params && params[1];
          if (!addrTd)
            throw makeEthRpcError(
              -32602,
              "Missing address for eth_signTypedData_v4"
            );
          if (typedData == null)
            throw makeEthRpcError(
              -32602,
              "Missing typedData for eth_signTypedData_v4"
            );
          var net4 = await fetchNetworkInfo();
          var typed_data_json =
            typeof typedData === "string"
              ? typedData
              : JSON.stringify(typedData);
          var respTd = await tauriInvoke("ipc_request", {
            request: {
              type: "SignTypedData",
              payload: {
                address: addrTd,
                typed_data_json: typed_data_json,
                chain_id: net4.chain_id,
              },
            },
          });
          if (respTd && respTd.type === "SignedTypedData")
            return respTd.payload;
          if (respTd && respTd.type === "SignedMessage") return respTd.payload;
          if (respTd && respTd.type === "Error") {
            throw mapIpcError(respTd.payload, 4100, "Signing failed");
          }
          throw makeEthRpcError(-32603, "Unexpected response from wallet");
        }

        if (m === "wallet_switchethereumchain") {
          var p0 = params && params[0];
          var swChainId = p0 && p0.chainId;
          if (!swChainId) throw makeEthRpcError(-32602, "Missing chainId");
          var decChainId;
          if (typeof swChainId === "string" && swChainId.startsWith("0x")) {
            decChainId = BigInt(swChainId).toString();
          } else {
            decChainId = BigInt(swChainId).toString();
          }
          var respSw = await tauriInvoke("ipc_request", {
            request: {
              type: "SwitchChain",
              payload: { chain_id: Number(decChainId) },
            },
          });
          if (respSw && respSw.type === "Error") {
            throw mapIpcError(respSw.payload, 4200, "Switch failed");
          }
          var net5 = await fetchNetworkInfo();
          currentChainIdHex = toHexChainId(net5.chain_id);
          emitter.emit("chainChanged", currentChainIdHex);
          syncLegacy();
          return null;
        }

        throw makeEthRpcError(4200, "Unsupported method: " + method);
      } catch (e) {
        if (e && (e.code === 4100 || e.code === 4900)) {
          emitDisconnect(e);
        }
        throw normalizeProviderError(e);
      }
    },
    on: function (event, handler) {
      emitter.on(event, handler);
    },
    addListener: function (event, handler) {
      emitter.on(event, handler);
    },
    removeListener: function (event, handler) {
      emitter.removeListener(event, handler);
    },
    off: function (event, handler) {
      emitter.removeListener(event, handler);
    },
  };

  try {
    ethereumProvider.providers = [ethereumProvider];
    ethereumProvider.provider = ethereumProvider;
  } catch (e) {}

  ethereumProvider.disconnect = async function () {
    clearDappSession({ code: 4900, message: "Disconnected from dApp" });
  };

  window.ethereum = ethereumProvider;

  // EIP-6963: announce twice — MetaMask tile (wagmi/uniswap) + explicit "Vaughan Wallet".
  // Same EIP-1193 provider; this WebView has no extension, so no conflict with real MetaMask.
  var providerInfoMetaMask = Object.freeze({
    uuid: "b90d9d3f-12a2-4c1b-b8f8-9b2e7f5a4c11",
    name: "MetaMask",
    icon: WALLET_ICON,
    rdns: "io.metamask",
  });
  var providerInfoVaughan = Object.freeze({
    uuid: "a7e68d1c-0c8b-4f3e-9d2a-1e2f3a4b5c6d",
    name: "Vaughan Wallet",
    icon: WALLET_ICON,
    rdns: "io.vaughan.wallet",
  });

  fetchNetworkInfo()
    .then(function (net) {
      currentChainIdHex = toHexChainId(net.chain_id);
      syncLegacy();
    })
    .catch(function () {
      syncLegacy();
    });

  function announceProvider() {
    try {
      var p = window.ethereum;
      [providerInfoMetaMask, providerInfoVaughan].forEach(function (info) {
        window.dispatchEvent(
          new CustomEvent("eip6963:announceProvider", {
            detail: Object.freeze({ info: info, provider: p }),
          })
        );
      });
    } catch (e) {
      console.error("Failed to announce EIP-6963 provider", e);
    }
  }

  function scheduleDiscoveryAnnouncements() {
    var delays = [0, 30, 100, 250, 600];
    for (var i = 0; i < delays.length; i++) {
      (function (ms) {
        setTimeout(announceProvider, ms);
      })(delays[i]);
    }
    function onVis() {
      if (document.visibilityState === "visible") announceProvider();
    }
    document.addEventListener("visibilitychange", onVis);
    window.addEventListener("focus", announceProvider);
    window.addEventListener("load", announceProvider);
    if (document.readyState === "complete") {
      announceProvider();
    } else {
      window.addEventListener("load", announceProvider);
    }
  }

  function onRequestProvider() {
    announceProvider();
  }
  window.addEventListener("eip6963:requestProvider", onRequestProvider, true);
  document.addEventListener("eip6963:requestProvider", onRequestProvider, true);
  scheduleDiscoveryAnnouncements();
  announceProvider();
  try {
    window.dispatchEvent(new Event("ethereum#initialized"));
  } catch (e) {}
  }

  if (needsTopWindowBridge()) {
    setTimeout(setup, 0);
  } else {
    waitForTauri(setup);
  }
})();

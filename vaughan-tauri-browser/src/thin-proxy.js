// thin-proxy.js — Injected into the dApp webview. Token-free, logic-free pipe.
(function() {
  if (window.ethereum) return;

  // Create hidden iframe pointing to our isolated provider page
  const iframe = document.createElement('iframe');
  iframe.src = 'vaughan://localhost/provider';
  iframe.style.cssText = 'display:none;width:1px;height:1px;position:absolute;left:-9999px;';
  
  const pending = new Map();
  let id = 0;

  window.ethereum = {
    request({ method, params }) {
      return new Promise((resolve, reject) => {
        const reqId = ++id;
        
        // Add a 60-second timeout for requests
        const timer = setTimeout(() => {
            if (pending.has(reqId)) {
                pending.delete(reqId);
                reject(new Error("Provider request timed out"));
            }
        }, 60000);

        pending.set(reqId, { resolve, reject, timer });
        iframe.contentWindow.postMessage(
          { type: 'VAUGHAN_REQUEST', id: reqId, method, params },
          '*' // Target origin
        );
      });
    },

    on(event, fn) {
      iframe.contentWindow.postMessage({ type: 'VAUGHAN_ON', event }, '*');
      window.addEventListener(`vaughan:${event}`, (e) => fn(e.detail));
    },

    removeListener(event, fn) {
      window.removeEventListener(`vaughan:${event}`, fn);
    },

    isVaughan: true,
    isMetaMask: true,
    _metamask: {
      isUnlocked() {
        return Promise.resolve(true);
      }
    },
    isConnected() { return true; },
    chainId: null,
    networkVersion: null,
    selectedAddress: null,
    _events: {}
  };

  // Handle responses and events from the isolated provider
  window.addEventListener('message', (e) => {
    // Validate source is exactly our hidden iframe
    if (e.source !== iframe.contentWindow) return;
    
    const { type, id: respId, result, error, event, payload } = e.data;
    
    if (type === 'VAUGHAN_RESPONSE') {
      const entry = pending.get(respId);
      if (entry) {
        clearTimeout(entry.timer);
        pending.delete(respId);
        if (error) {
          const ethErr = new Error(error.message || String(error));
          ethErr.code = error.code || -32603;
          entry.reject(ethErr);
        } else {
          entry.resolve(result);
        }
      }
    }
    
    if (type === 'VAUGHAN_EVENT') {
      if (event === 'chainChanged') {
        window.ethereum.chainId = payload;
        window.ethereum.networkVersion = parseInt(payload, 16).toString();
      } else if (event === 'accountsChanged') {
        window.ethereum.selectedAddress = (payload && payload.length > 0) ? payload[0] : null;
      }
      window.dispatchEvent(new CustomEvent(`vaughan:${event}`, { detail: payload }));
    }
  });

  const announce = () => {
    window.dispatchEvent(new CustomEvent('eip6963:announceProvider', {
      detail: Object.freeze({
        info: {
          uuid: 'io.metamask',
          name: 'MetaMask',
          icon: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAxMDAgMTAwIj48Y2lyY2xlIGN4PSI1MCIgY3k9IjUwIiByPSI1MCIgZmlsbD0iI2Y2ODUxYiIvPjwvc3ZnPg==',
          rdns: 'io.metamask'
        },
        provider: window.ethereum
      })
    }));
  };

  // EIP-6963 request listener
  window.addEventListener('eip6963:requestProvider', announce);

  // eip-6963 announce and initial state sync
  iframe.addEventListener('load', () => {
    // Request initial state to populate synchronous properties
    window.ethereum.request({ method: 'eth_chainId' }).then(chain => {
      window.ethereum.chainId = chain;
      window.ethereum.networkVersion = parseInt(chain, 16).toString();
    }).catch(() => {});

    window.ethereum.request({ method: 'eth_accounts' }).then(accs => {
      window.ethereum.selectedAddress = (accs && accs.length > 0) ? accs[0] : null;
    }).catch(() => {});

    announce();
  });

  (document.head || document.documentElement).appendChild(iframe);
})();

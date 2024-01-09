import {
  CallZomeRequest,
  CallZomeRequestSigned,
  CallZomeRequestUnsigned,
  getNonceExpiration,
  randomNonce,
} from "@holochain/client";
import { encode } from "@msgpack/msgpack";
import { core } from "@tauri-apps/api";

// Here we are trying to cover all platforms in different ways
// Windows doesn't support requests of type happ://APPID
// MacOs doesn't support requests of type http://APPID.localhost:4040
export enum IframeProtocol {
  Assets,
  LocalhostSubdomain,
  LocaltestMe,
}

async function fetchPing(origin: string) {
  const iframe = document.createElement("iframe");
  iframe.src = origin;
  iframe.style.display = "none";
  document.body.appendChild(iframe);

  return new Promise((resolve, reject) => {
    let resolved = false;

    const listener = (message: any) => {
      if (message.source === iframe.contentWindow) {
        resolved = true;
        document.body.removeChild(iframe);
        window.removeEventListener("message", listener);
        resolve(null);
      }
    };
    setTimeout(() => {
      if (resolved) return;
      document.body.removeChild(iframe);
      window.removeEventListener("message", listener);
      reject(new Error("Protocol failed to start."));
    }, 1000);

    window.addEventListener("message", listener);
  });
}

export function isWindows(): boolean {
  return navigator.appVersion.includes("Win");
}

async function getIframeProtocol(httpServerPort: number) {
  if (isWindows()) {
    try {
      await fetchPing(`http://ping.localhost:${httpServerPort}`);
      return IframeProtocol.LocalhostSubdomain;
    } catch (e) {
      return IframeProtocol.LocaltestMe;
    }
  } else {
    try {
      await fetchPing("happ://ping");
      return IframeProtocol.Assets;
    } catch (e) {
      try {
        await fetchPing(`http://ping.localhost:${httpServerPort}`);
        return IframeProtocol.LocalhostSubdomain;
      } catch (e) {
        return IframeProtocol.LocaltestMe;
      }
    }
  }
}

export function appOrigin(
  iframeProtocol: IframeProtocol,
  appId: string,
  httpServerPort: number
): string {
  if (iframeProtocol === IframeProtocol.Assets) {
    return `happ://${appId}`;
  } else if (iframeProtocol === IframeProtocol.LocalhostSubdomain) {
    return `http://${appId}.localhost:${httpServerPort}`;
  } else {
    return `http://${appId}.localtest.me:${httpServerPort}`;
  }
}

function getAppIdFromOrigin(
  iframeProtocol: IframeProtocol,
  origin: string
): string {
  if (iframeProtocol === IframeProtocol.Assets) {
    return origin.split("://")[1].split("?")[0].split("/")[0];
  } else {
    return origin.split("://")[1].split("?")[0].split(".")[0];
  }
}

export interface RuntimeInfo {
  http_server_port: number;
  app_port: number;
  admin_port: number;
}

const appId = (window as any).__APP_ID__;

core
  .invoke<RuntimeInfo>("plugin:holochain|get_runtime_info", {})
  .then((runtimeInfo: RuntimeInfo) => {
    getIframeProtocol(runtimeInfo.http_server_port).then((protocol) => {
      window.addEventListener("message", async (message) => {
        const appId = getAppIdFromOrigin(protocol, message.origin);

        const response = await handleRequest(runtimeInfo, appId, message.data);
        message.ports[0].postMessage({ type: "success", result: response });
      });
      buildFrame(runtimeInfo, protocol, appId);
    });
  });

export type Request =
  | {
      type: "sign-zome-call";
      zomeCall: CallZomeRequest;
    }
  | {
      type: "get-app-runtime-info";
    }
  | {
      type: "get-locales";
    };

async function handleRequest(
  runtimeInfo: RuntimeInfo,
  appId: string,
  request: Request
) {
  switch (request.type) {
    case "get-app-runtime-info":
      return {
        appId,
        runtimeInfo,
      };
    case "sign-zome-call":
      return signZomeCallTauri(request.zomeCall);
    case "get-locales":
      return core.invoke("plugin:holochain|get_locales", {});
  }
}

function buildFrame(
  runtimeInfo: RuntimeInfo,
  iframeProtocol: IframeProtocol,
  appId: string
) {
  const iframe = document.createElement("iframe");
  const origin = appOrigin(iframeProtocol, appId, runtimeInfo.http_server_port);

  iframe.src = `${origin}${window.location.search}`;
  iframe.frameBorder = "0";
  document.body.appendChild(iframe);
}

type TauriByteArray = number[]; // Tauri requires a number array instead of a Uint8Array

interface CallZomeRequestSignedTauri
  extends Omit<
    CallZomeRequestSigned,
    "cap_secret" | "cell_id" | "provenance" | "nonce"
  > {
  cell_id: [TauriByteArray, TauriByteArray];
  provenance: TauriByteArray;
  nonce: TauriByteArray;
  expires_at: number;
}

interface CallZomeRequestUnsignedTauri
  extends Omit<
    CallZomeRequestUnsigned,
    "cap_secret" | "cell_id" | "provenance" | "nonce"
  > {
  cell_id: [TauriByteArray, TauriByteArray];
  provenance: TauriByteArray;
  nonce: TauriByteArray;
  expires_at: number;
}

export const signZomeCallTauri = async (request: CallZomeRequest) => {
  const zomeCallUnsigned: CallZomeRequestUnsignedTauri = {
    provenance: Array.from(request.provenance),
    cell_id: [Array.from(request.cell_id[0]), Array.from(request.cell_id[1])],
    zome_name: request.zome_name,
    fn_name: request.fn_name,
    payload: Array.from(encode(request.payload)),
    nonce: Array.from(await randomNonce()),
    expires_at: getNonceExpiration(),
  };

  const signedZomeCallTauri: CallZomeRequestSignedTauri = await core.invoke(
    "plugin:holochain|sign_zome_call",
    {
      zomeCallUnsigned,
    }
  );

  const signedZomeCall: CallZomeRequestSigned = {
    provenance: Uint8Array.from(signedZomeCallTauri.provenance),
    cap_secret: null,
    cell_id: [
      Uint8Array.from(signedZomeCallTauri.cell_id[0]),
      Uint8Array.from(signedZomeCallTauri.cell_id[1]),
    ],
    zome_name: signedZomeCallTauri.zome_name,
    fn_name: signedZomeCallTauri.fn_name,
    payload: Uint8Array.from(signedZomeCallTauri.payload),
    signature: Uint8Array.from(signedZomeCallTauri.signature),
    expires_at: signedZomeCallTauri.expires_at,
    nonce: Uint8Array.from(signedZomeCallTauri.nonce),
  };

  return signedZomeCall;
};

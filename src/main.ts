import {
  CallZomeRequest,
  CallZomeRequestSigned,
  CallZomeRequestUnsigned,
  getNonceExpiration,
  randomNonce,
} from "@holochain/client";
import { encode } from "@msgpack/msgpack";
import { primitives } from "@tauri-apps/api";

window.addEventListener("message", async (message) => {
  const appId = message.origin.split("://")[1].split("?")[0].split(".")[0];

  const response = await handleRequest(appId, message.data);
  message.ports[0].postMessage({ type: "success", result: response });
});

export type Request =
  | {
      type: "sign-zome-call";
      zomeCall: CallZomeRequest;
    }
  | {
      type: "get-locales";
    };

async function handleRequest(_appId: string, request: Request) {
  switch (request.type) {
    case "sign-zome-call":
      return signZomeCallTauri(request.zomeCall);
    case "get-locales":
      return primitives.invoke("plugin:holochain|get_locales", {});
  }
}

const env = (window as any).__HC_LAUNCHER_ENV__;
const appId = env.INSTALLED_APP_ID;
const httpServerPort = env.HTTP_SERVER_PORT;

const iframe = document.createElement("iframe");
iframe.src = `http://${appId}.localhost:${httpServerPort}`;
iframe.frameBorder = "0";
document.body.appendChild(iframe);

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

  const signedZomeCallTauri: CallZomeRequestSignedTauri =
    await primitives.invoke("plugin:holochain|sign_zome_call", {
      zomeCallUnsigned,
    });

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

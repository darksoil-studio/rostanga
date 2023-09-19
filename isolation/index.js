// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

window.__TAURI_ISOLATION_HOOK__ = (payload, options) => {
  if (payload.cmd === "sign_zome_call") {
    payload.cmd = "plugin:holochain|sign_zome_call";
  }
  if (payload.zomeCallUnsigned) {
    payload.payload = {
      zomeCallUnsigned: payload.zomeCallUnsigned,
    };
  }
  return payload;
};

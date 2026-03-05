import { invoke } from "@tauri-apps/api/core";

import { HashWithPaths, ScanStatusDto } from "../types";

export const startScan = async (rootPath: string): Promise<void> => {
  await invoke("start_scan", { rootPath });
};

export const getHashesWithPaths = async (): Promise<HashWithPaths[]> => {
  return invoke<HashWithPaths[]>("get_hashes_with_paths");
};

export const getScanStatus = async (): Promise<ScanStatusDto> => {
  return invoke<ScanStatusDto>("get_scan_status");
};

import { invoke } from "@tauri-apps/api/core";

import { BackupTreeNode, BackupTreeQuery, ScanRootOption, ScanStatusDto } from "../types";

export const startScan = async (rootPath: string): Promise<void> => {
  await invoke("start_scan", { rootPath });
};

export const listScanRoots = async (): Promise<ScanRootOption[]> => {
  return invoke<ScanRootOption[]>("list_scan_roots");
};

export const getBackupTree = async (query: BackupTreeQuery): Promise<BackupTreeNode[]> => {
  return invoke<BackupTreeNode[]>("get_backup_tree", { query });
};

export const getScanStatus = async (): Promise<ScanStatusDto> => {
  return invoke<ScanStatusDto>("get_scan_status");
};

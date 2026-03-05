export interface HashWithPaths {
  fullHash: string;
  fingerprintHash: string;
  fileSize: number;
  paths: string[];
  occurrenceCount: number;
}

export interface ScanStatusDto {
  isRunning: boolean;
  rootPath: string | null;
  totalCandidates: number;
  scannedFiles: number;
  hashedFiles: number;
  errorCount: number;
  lastError: string | null;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface ScanStartedEvent {
  rootPath: string;
  scanId: number;
  totalCandidates: number;
}

export interface ScanProgressEvent {
  scanId: number;
  path: string;
  scannedFiles: number;
  totalCandidates: number;
  hashedFiles: number;
  errorCount: number;
}

export interface ScanCompletedEvent {
  scanId: number;
  rootPath: string;
  scannedFiles: number;
  hashedFiles: number;
  errorCount: number;
  removedPaths: number;
  finishedAt: string;
}

export interface ScanErrorEvent {
  scanId: number;
  path: string | null;
  message: string;
}

/**
 * Counting a folder before anything reads it.
 *
 * This runs entirely in the browser on the file list the picker hands back:
 * names, sizes and dates only, never contents. It is the honest version of the
 * "preview before analysis" screen — the numbers on it are the owner's own
 * folder, not a sample.
 */

/** The file types Knowlith can read. Matches the Rust extractor exactly. */
const READABLE = new Set(["md", "markdown", "txt", "text", "csv", "tsv", "xlsx", "xlsm", "docx", "pdf"])

/** Names that are never worth reading, recognised without opening anything. */
function isNoise(name: string): boolean {
  return (
    name.startsWith("~$") ||
    name.startsWith(".") ||
    name.toLowerCase() === "thumbs.db" ||
    name.toLowerCase() === "desktop.ini"
  )
}

/**
 * Filenames people use when they keep the previous version next to the
 * current one. Getting this wrong in either direction is visible: miss them
 * and last year's prices come back as current, flag too eagerly and the owner
 * stops trusting the count.
 */
const OLD_VERSION = /(^|[ _\-(])(v|ver|verzija|rev)[ _.]?\d+|(^|[ _\-(])(old|stari|stara|staro|kopija|copy|backup|arhiva|archive)([ _\-.)]|$)|\(\d+\)\./i

export interface FileTypeCount {
  ext: string
  count: number
}

export interface Inventory {
  /** What the picker was pointed at, as far as the browser will say. */
  folderName: string
  fileCount: number
  bytes: number
  /** Readable types, largest group first. */
  fileTypes: FileTypeCount[]
  duplicates: number
  oldVersions: number
  unsupported: number
  /** Most recent modification time across the folder, or null. */
  newest: string | null
}

/**
 * Builds the inventory from the browser's file list.
 *
 * Duplicates are counted by name and size together, because that is the pair
 * that survives a copy between folders; two genuinely different files almost
 * never share both.
 */
export function inventoryOf(files: File[]): Inventory {
  const seen = new Map<string, number>()
  const types = new Map<string, number>()
  let bytes = 0
  let counted = 0
  let duplicates = 0
  let oldVersions = 0
  let unsupported = 0
  let newest = 0

  for (const file of files) {
    const name = file.name
    if (isNoise(name)) continue

    counted += 1
    bytes += file.size
    if (file.lastModified > newest) newest = file.lastModified

    const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1).toLowerCase() : ""
    if (READABLE.has(ext)) {
      const label = ext.toUpperCase()
      types.set(label, (types.get(label) ?? 0) + 1)
    } else {
      unsupported += 1
    }

    const key = `${name.toLowerCase()}:${file.size}`
    const before = seen.get(key) ?? 0
    seen.set(key, before + 1)
    if (before > 0) duplicates += 1

    if (OLD_VERSION.test(name)) oldVersions += 1
  }

  const relative = files[0]?.webkitRelativePath ?? ""
  const folderName = relative.split("/")[0] || "Selected folder"

  return {
    folderName,
    fileCount: counted,
    bytes,
    fileTypes: [...types.entries()]
      .map(([ext, count]) => ({ ext, count }))
      .sort((a, b) => b.count - a.count),
    duplicates,
    oldVersions,
    unsupported,
    newest: newest ? new Date(newest).toISOString() : null,
  }
}

/** The demo folder, for anyone who wants to see the flow before pointing it at their own files. */
export const demoInventory: Inventory = {
  folderName: "Termoval — Prodaja",
  fileCount: 2438,
  bytes: 9_020_000_000,
  fileTypes: [
    { ext: "PDF", count: 1113 },
    { ext: "DOCX", count: 730 },
    { ext: "XLSX", count: 351 },
    { ext: "CSV", count: 90 },
  ],
  duplicates: 312,
  oldVersions: 71,
  unsupported: 154,
  newest: new Date(Date.now() - 36 * 3600 * 1000).toISOString(),
}

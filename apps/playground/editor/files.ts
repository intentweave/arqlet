// Host file handling for references: a download of the canonical text, and reading a chosen file as text.

/** Offers `text` as a file download. The name is the caller's; nothing is sent anywhere. */
export function downloadText(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], {type: 'application/json'}));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** The text of a chosen file. Oversized files are refused before they are read. */
export async function readFileText(file: File, limitBytes = 64_000_000): Promise<string> {
  if (file.size > limitBytes) throw Error('The file is too large.');
  return file.text();
}

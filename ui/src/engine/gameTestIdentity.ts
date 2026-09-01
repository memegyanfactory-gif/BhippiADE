/** Only this one-way identity may leave a worker client; the raw nonce stays private. */
export async function sha256SessionIdentity(nonce: string): Promise<string> {
  const bytes = new TextEncoder().encode(nonce);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `sha256:${hex}`;
}

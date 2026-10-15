interface Manifest { version?: string; optional_permissions?: string[]; permissions?: string[] }
export async function requestNavigationAccess(api: { manifest(): Manifest; request(): Promise<boolean> }): Promise<void> {
  const manifest = api.manifest();
  if (![...(manifest.optional_permissions ?? []), ...(manifest.permissions ?? [])].includes("webNavigation")) throw new Error(`Chrome đang nạp extension v${manifest.version ?? "?"} chưa khai báo quyền liên kết iframe. Đóng recorder cũ, Reload đúng extension local rồi mở lại; không cần gỡ extension.`);
  try {
    if (!await api.request()) throw new Error("Chưa cấp quyền liên kết iframe; chưa thể xác minh vùng thu.");
  } catch (error) {
    if (String(error).includes("Only permissions specified in the manifest")) throw new Error(`Chrome chưa cập nhật quyền của extension v${manifest.version ?? "?"}. Đóng recorder, Reload đúng extension rồi mở lại. Nếu vẫn lỗi, gửi ảnh phiên bản và ID trên chrome://extensions.`);
    throw error;
  }
}

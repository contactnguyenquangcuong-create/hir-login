/// Flat rectangular country flag (flag-icons sprite); empty input renders nothing.
export function CountryFlag({ cc, height = 17 }: { cc: string; height?: number }) {
  if (!cc || cc.length !== 2 || !/^[a-zA-Z]{2}$/.test(cc)) return null;
  const code = cc.toLowerCase();
  // `fi fi-XX`; omit `fis` to keep 4:3 rectangle.
  return (
    <span
      className={`fi fi-${code} rounded-[3px] shadow-[inset_0_0_0_1px_#00000033] flex-none inline-block bg-cover bg-center`}
      style={{ height, width: Math.round(height * 4 / 3) }}
      aria-hidden
    />
  );
}

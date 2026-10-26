export const datePatternPreview = (input = '', now = new Date()) => {
  const pattern = input === 'yyyymmdd' ? 'yyyyMMdd' : input;
  const hour = now.getHours();
  const pad = (value: number) => String(value).padStart(2, '0');
  const values: Record<string, string> = {
    yyyy: String(now.getFullYear()),
    yy: String(now.getFullYear()).slice(-2),
    MM: pad(now.getMonth() + 1),
    dd: pad(now.getDate()),
    HH: pad(hour),
    hh: pad(hour % 12 || 12),
    mm: pad(now.getMinutes()),
    ss: pad(now.getSeconds()),
    a: hour >= 12 ? 'PM' : 'AM',
  };
  return pattern.replace(
    /yyyy|yy|MM|dd|HH|hh|mm|ss|a/g,
    (token) => values[token],
  );
};

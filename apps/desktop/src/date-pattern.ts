export const datePatternPreview = (input = '') => {
  const pattern = input === 'yyyymmdd' ? 'yyyyMMdd' : input;
  const values: Record<string, string> = {
    yyyy: '2026',
    yy: '26',
    MM: '10',
    dd: '23',
    HH: '17',
    hh: '05',
    mm: '42',
    ss: '09',
    a: 'PM',
  };
  return pattern.replace(
    /yyyy|yy|MM|dd|HH|hh|mm|ss|a/g,
    (token) => values[token],
  );
};

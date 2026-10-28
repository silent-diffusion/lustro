/* Shared shell for the prototype: nav, theme, icons, formatting, layout helpers. */
(function () {
  const Lustro = (window.Lustro = window.Lustro || {});

  const I = {
    // The terrain view in miniature: a contour-sliced hill on a disc, seen at an angle. Each contour is filled with the
    // background so higher levels hide the lines behind them. The summit uses the accent, like a selected contour.
    logo: '<svg viewBox="0 0 32 32"><g stroke="currentColor" stroke-width="1.0" stroke-linejoin="round"><path d="M29.3 23.7 28.6 24.3 27.8 24.9 26.9 25.5 25.9 26 24.7 26.4 23.5 26.8 22.2 27.2 20.9 27.4 19.5 27.6 18.1 27.8 16.6 27.8 15.2 27.8 13.7 27.8 12.3 27.6 10.9 27.4 9.6 27.1 8.3 26.8 7.1 26.3 6 25.9 4.9 25.4 4 24.8 3.2 24.2 2.6 23.5 2 22.9 1.6 22.2 1.3 21.5 1.2 20.7 1.2 20 1.4 19.3 1.7 18.6 2.1 17.9 2.7 17.2 3.4 16.6 4.2 16 5.1 15.4 6.1 14.9 7.3 14.5 8.5 14.1 9.8 13.7 11.1 13.4 12.5 13.2 13.9 13.1 15.4 13 16.8 13 18.3 13.1 19.7 13.3 21.1 13.5 22.4 13.8 23.7 14.1 24.9 14.5 26 15 27.1 15.5 28 16.1 28.8 16.7 29.4 17.3 30 18 30.4 18.7 30.7 19.4 30.8 20.1 30.8 20.9 30.6 21.6 30.3 22.3 29.9 23Z" fill="currentColor" fill-opacity=".35" stroke="none"/><path d="M29.3 21.9 28.6 22.5 27.8 23.1 26.9 23.7 25.9 24.2 24.7 24.6 23.5 25 22.2 25.4 20.9 25.7 19.5 25.9 18.1 26 16.6 26.1 15.2 26.1 13.7 26 12.3 25.8 10.9 25.6 9.6 25.3 8.3 25 7.1 24.6 6 24.1 4.9 23.6 4 23 3.2 22.4 2.6 21.8 2 21.1 1.6 20.4 1.3 19.7 1.2 19 1.2 18.2 1.4 17.5 1.7 16.8 2.1 16.1 2.7 15.4 3.4 14.8 4.2 14.2 5.1 13.6 6.1 13.1 7.3 12.7 8.5 12.3 9.8 11.9 11.1 11.7 12.5 11.5 13.9 11.3 15.4 11.3 16.8 11.3 18.3 11.3 19.7 11.5 21.1 11.7 22.4 12 23.7 12.3 24.9 12.7 26 13.2 27.1 13.7 28 14.3 28.8 14.9 29.4 15.5 30 16.2 30.4 16.9 30.7 17.6 30.8 18.4 30.8 19.1 30.6 19.8 30.3 20.5 29.9 21.2Z" fill="var(--bg)"/><path d="M22.5 12.4 22.3 12.4 22 12.3 21.7 12.3 21.5 12.3 21.3 12.3 21 12.2 20.8 12.2 20.5 12.2 20.2 12.2 20 12.2 19.8 12.2 19.5 12.2 19.3 12.2 19.1 12.2 18.9 12.2 18.7 12.3 18.5 12.3 18.2 12.3 18 12.3 17.8 12.4 17.5 12.4 17.3 12.4 17.1 12.5 16.9 12.5 16.7 12.6 16.6 12.6 16.4 12.7 16.1 12.7 15.9 12.8 15.7 12.8 15.5 12.9 15.3 12.9 15.1 13 15 13.1 14.7 13.1 14.5 13.2 14.3 13.3 14.2 13.3 14 13.4 13.8 13.4 13.6 13.5 13.3 13.5 13.1 13.6 12.9 13.6 12.7 13.6 12.5 13.7 12.3 13.7 12.1 13.7 11.8 13.7 11.5 13.7 11.3 13.7 11.1 13.7 10.9 13.7 10.6 13.7 10.4 13.7 10.2 13.7 9.9 13.7 9.6 13.7 9.4 13.8 9.2 13.8 9 13.8 8.8 13.8 8.6 13.8 8.3 13.8 8.1 13.9 7.9 13.9 7.7 14 7.5 14 7.3 14 7.1 14.1 6.9 14.1 6.7 14.2 6.5 14.3 6.3 14.4 6.2 14.4 6 14.5 5.7 14.6 5.6 14.7 5.4 14.8 5.2 14.9 5.1 15 5 15.1 4.8 15.2 4.7 15.4 4.6 15.5 4.5 15.6 4.4 15.7 4.3 15.9 4.3 16 4.2 16.1 4.2 16.3 4.1 16.4 4.1 16.5 4.1 16.6 4.1 16.7 4.1 16.8 4.2 17 4.2 17.1 4.2 17.2 4.3 17.3 4.4 17.4 4.4 17.5 4.5 17.6 4.6 17.8 4.7 17.9 4.8 18 4.9 18.1 5 18.1 5.1 18.2 5.2 18.3 5.4 18.4 5.5 18.5 5.7 18.6 5.9 18.7 6 18.8 6.2 18.9 6.4 19 6.6 19 6.8 19.1 7.1 19.2 7.3 19.3 7.5 19.4 7.7 19.4 7.9 19.5 8.2 19.6 8.4 19.7 8.6 19.8 8.7 19.9 8.9 19.9 9 20.1 9.2 20.2 9.2 20.3 9.3 20.4 9.4 20.4 9.5 20.5 9.6 20.6 9.7 20.7 9.7 20.8 9.8 20.9 9.9 21 10 21.2 10.2 21.3 10.3 21.4 10.4 21.5 10.5 21.6 10.7 21.6 10.9 21.7 11 21.8 11.2 21.9 11.4 22 11.7 22.1 11.9 22.2 12.2 22.2 12.5 22.3 12.8 22.4 13.1 22.4 13.4 22.4 13.6 22.4 13.9 22.5 14.1 22.5 14.3 22.5 14.5 22.5 14.8 22.4 15 22.4 15.2 22.4 15.5 22.4 15.7 22.3 15.9 22.3 16.1 22.2 16.2 22.2 16.4 22.1 16.7 22.1 16.9 22 17.1 21.9 17.2 21.8 17.4 21.8 17.6 21.7 17.7 21.6 17.9 21.5 18.1 21.4 18.3 21.3 18.4 21.2 18.6 21.1 18.8 21 19 20.9 19.1 20.9 19.4 20.8 19.6 20.7 19.8 20.7 19.9 20.6 20.1 20.6 20.3 20.5 20.5 20.5 20.7 20.5 20.9 20.4 21.2 20.4 21.4 20.3 21.6 20.3 21.8 20.3 22.1 20.2 22.3 20.2 22.5 20.2 22.7 20.1 22.9 20.1 23.1 20.1 23.3 20 23.5 20 23.7 19.9 23.9 19.8 24.2 19.8 24.4 19.7 24.5 19.7 24.7 19.6 24.9 19.5 25.1 19.4 25.3 19.4 25.4 19.3 25.7 19.2 25.8 19.1 26 19 26.2 18.9 26.3 18.8 26.4 18.7 26.6 18.6 26.8 18.5 27 18.3 27.1 18.2 27.2 18 27.4 17.9 27.5 17.8 27.6 17.6 27.7 17.4 27.8 17.3 27.8 17.2 27.8 17.1 27.9 17 27.9 16.9 27.9 16.7 28 16.6 28 16.5 28 16.3 28 16.2 28 16.1 28 16 27.9 15.9 27.9 15.7 27.9 15.6 27.8 15.5 27.8 15.4 27.7 15.3 27.7 15.2 27.6 15.1 27.6 15 27.5 14.9 27.4 14.8 27.3 14.7 27.2 14.6 27.1 14.5 27 14.4 26.9 14.3 26.8 14.2 26.7 14.1 26.6 14 26.4 13.9 26.3 13.8 26.1 13.7 26 13.6 25.8 13.5 25.6 13.4 25.5 13.3 25.3 13.2 25.1 13.2 24.9 13.1 24.7 13 24.4 12.9 24.2 12.8 24.1 12.8 23.8 12.7 23.5 12.6 23.2 12.6 22.9 12.5 22.7 12.4Z" fill="var(--bg)"/><path d="M22.4 11.3 22.2 11.2 21.9 11.2 21.6 11.1 21.3 11.1 21.1 11.1 20.8 11.1 20.6 11.1 20.4 11.1 20.1 11.1 19.8 11.1 19.6 11.1 19.5 11.1 19.2 11.1 19 11.1 18.7 11.1 18.5 11.2 18.3 11.2 18 11.2 17.8 11.3 17.6 11.3 17.5 11.4 17.3 11.4 17.1 11.5 16.9 11.5 16.7 11.6 16.5 11.6 16.2 11.7 16.1 11.8 15.9 11.8 15.7 11.9 15.5 12 15.3 12.1 15.2 12.1 15 12.2 14.8 12.3 14.6 12.3 14.4 12.4 14.2 12.5 14 12.5 13.8 12.6 13.5 12.7 13.3 12.7 13.1 12.7 12.9 12.7 12.7 12.7 12.5 12.7 12.2 12.7 12 12.7 11.8 12.7 11.5 12.7 11.3 12.7 11 12.7 10.7 12.7 10.5 12.6 10.2 12.6 10 12.6 9.8 12.6 9.6 12.6 9.3 12.6 9.1 12.7 8.8 12.7 8.6 12.7 8.4 12.7 8.2 12.8 8 12.8 7.8 12.8 7.5 12.9 7.4 13 7.2 13 7 13.1 6.8 13.2 6.6 13.2 6.5 13.3 6.3 13.4 6.1 13.5 6 13.6 5.9 13.8 5.7 13.9 5.6 14 5.5 14.2 5.5 14.3 5.4 14.4 5.4 14.5 5.4 14.7 5.3 14.8 5.3 14.9 5.4 15 5.4 15.1 5.4 15.3 5.5 15.4 5.6 15.5 5.7 15.6 5.7 15.7 5.8 15.8 5.9 15.9 6 15.9 6.2 16 6.3 16.2 6.5 16.2 6.7 16.3 6.9 16.4 7 16.5 7.2 16.6 7.5 16.7 7.8 16.8 8.1 16.8 8.3 16.9 8.6 17 8.9 17 9.2 17.1 9.5 17.1 9.8 17.2 10.1 17.3 10.4 17.4 10.6 17.4 10.7 17.5 10.9 17.6 11 17.7 11.1 17.8 11.1 17.9 11.2 18.1 11.2 18.2 11.2 18.3 11.3 18.4 11.3 18.5 11.4 18.6 11.4 18.7 11.5 18.8 11.5 18.9 11.6 19 11.7 19.1 11.9 19.2 12 19.3 12.2 19.4 12.4 19.5 12.6 19.6 12.8 19.6 13 19.7 13.3 19.8 13.5 19.8 13.8 19.8 14 19.8 14.3 19.8 14.5 19.8 14.7 19.8 14.9 19.7 15.1 19.7 15.4 19.6 15.6 19.6 15.7 19.5 15.9 19.5 16.1 19.4 16.3 19.3 16.4 19.1 16.6 19.1 16.7 19 16.9 18.8 17 18.7 17.2 18.6 17.4 18.5 17.5 18.4 17.7 18.3 17.9 18.2 18 18.1 18.2 18.1 18.4 18.1 18.6 18 18.8 18 19.1 18 19.3 17.9 19.6 17.9 19.8 17.9 20.1 17.9 20.3 17.8 20.5 17.8 20.7 17.8 20.9 17.8 21.1 17.8 21.3 17.7 21.6 17.7 21.8 17.7 22 17.6 22.3 17.6 22.5 17.6 22.7 17.5 22.9 17.5 23.1 17.5 23.2 17.4 23.4 17.4 23.6 17.3 23.9 17.2 24.1 17.1 24.2 17.1 24.4 17 24.6 16.9 24.8 16.8 24.9 16.8 25.2 16.7 25.3 16.6 25.4 16.5 25.6 16.4 25.8 16.2 26 16.1 26.1 15.9 26.2 15.8 26.4 15.6 26.5 15.5 26.6 15.3 26.6 15.2 26.7 15.1 26.7 15 26.7 14.8 26.8 14.7 26.8 14.6 26.8 14.5 26.8 14.4 26.8 14.3 26.7 14.1 26.7 14 26.7 13.9 26.7 13.8 26.6 13.7 26.6 13.6 26.5 13.5 26.4 13.4 26.4 13.3 26.3 13.2 26.2 13.1 26.1 13 26 12.8 25.8 12.7 25.7 12.6 25.6 12.5 25.5 12.5 25.4 12.4 25.2 12.3 25.1 12.2 24.9 12.1 24.7 12 24.5 11.9 24.4 11.8 24.1 11.7 23.9 11.6 23.6 11.6 23.4 11.5 23.2 11.4 23 11.4 22.7 11.3 22.4 11.3Z" fill="var(--bg)"/><path d="M21.5 9.8 21.3 9.8 21 9.8 20.8 9.7 20.6 9.7 20.3 9.7 20.1 9.7 19.9 9.7 19.6 9.8 19.4 9.8 19.1 9.8 18.9 9.8 18.7 9.9 18.5 9.9 18.2 9.9 18.1 10 17.9 10 17.7 10.1 17.5 10.1 17.3 10.2 17.1 10.3 16.9 10.3 16.7 10.4 16.6 10.4 16.4 10.5 16.1 10.6 16 10.7 15.8 10.8 15.6 10.9 15.5 11 15.3 11.1 15.1 11.1 14.9 11.2 14.7 11.3 14.6 11.4 14.4 11.5 14.2 11.6 14 11.6 13.9 11.7 13.7 11.7 13.4 11.8 13.2 11.8 12.9 11.8 12.6 11.7 12.4 11.7 12.2 11.7 11.9 11.6 11.6 11.6 11.3 11.5 11 11.5 10.9 11.5 10.6 11.4 10.3 11.4 10 11.4 9.8 11.4 9.6 11.4 9.4 11.4 9.1 11.4 8.8 11.4 8.6 11.4 8.4 11.5 8.2 11.5 8 11.6 7.8 11.6 7.7 11.7 7.5 11.7 7.3 11.8 7.1 11.9 7 12 6.8 12.1 6.7 12.3 6.5 12.4 6.5 12.6 6.4 12.7 6.4 12.8 6.4 12.9 6.4 13 6.4 13.2 6.4 13.3 6.5 13.4 6.6 13.5 6.7 13.6 6.8 13.7 6.9 13.8 7 13.9 7.1 14 7.3 14.1 7.5 14.2 7.7 14.2 7.9 14.3 8.2 14.4 8.5 14.4 8.8 14.5 9 14.5 9.2 14.6 9.5 14.6 9.8 14.6 10 14.6 10.2 14.6 10.5 14.6 10.7 14.6 10.9 14.6 11.1 14.5 11.3 14.5 11.6 14.5 11.8 14.4 12 14.4 12.2 14.3 12.4 14.3 12.6 14.2 12.8 14.2 13 14.1 13.2 14.1 13.4 14.1 13.7 14.1 13.9 14.1 14.1 14.1 14.3 14.2 14.5 14.2 14.7 14.3 14.9 14.4 15 14.5 15.3 14.6 15.4 14.7 15.6 14.8 15.8 14.9 15.9 14.9 16.2 15 16.4 15.1 16.7 15.2 17 15.2 17.3 15.3 17.5 15.3 17.7 15.3 18 15.4 18.3 15.4 18.6 15.4 18.8 15.4 19 15.4 19.3 15.4 19.5 15.4 19.7 15.4 20 15.4 20.3 15.4 20.5 15.4 20.7 15.4 20.9 15.4 21.1 15.4 21.4 15.4 21.6 15.3 21.9 15.3 22.1 15.3 22.3 15.2 22.5 15.2 22.7 15.1 22.9 15.1 23 15.1 23.2 15 23.4 14.9 23.7 14.9 23.8 14.8 24 14.7 24.2 14.6 24.4 14.5 24.5 14.4 24.7 14.3 24.8 14.3 25 14.1 25.2 14 25.3 13.9 25.4 13.7 25.5 13.6 25.6 13.4 25.7 13.3 25.8 13.2 25.8 13 25.8 12.9 25.9 12.8 25.9 12.7 25.9 12.6 25.9 12.5 25.9 12.4 25.8 12.3 25.8 12.1 25.7 12 25.7 11.9 25.6 11.8 25.6 11.6 25.5 11.5 25.4 11.4 25.3 11.3 25.2 11.3 25.1 11.2 25 11.1 24.9 11 24.7 10.9 24.6 10.8 24.4 10.7 24.3 10.6 24.1 10.5 23.9 10.4 23.7 10.3 23.5 10.3 23.3 10.2 23 10.1 22.7 10 22.4 10 22.2 9.9 21.8 9.9 21.6 9.8Z" fill="var(--bg)"/><path d="M21.3 8.4 21.1 8.4 20.8 8.4 20.6 8.3 20.4 8.3 20.1 8.3 19.9 8.4 19.7 8.4 19.5 8.4 19.3 8.4 19.1 8.4 18.9 8.5 18.6 8.5 18.4 8.6 18.2 8.6 17.9 8.7 17.7 8.7 17.6 8.8 17.4 8.9 17.2 9 17 9 16.9 9.1 16.7 9.2 16.5 9.3 16.4 9.4 16.2 9.5 16 9.7 15.9 9.7 15.8 9.8 15.6 10 15.5 10.1 15.4 10.3 15.3 10.4 15.2 10.5 15.2 10.6 15.2 10.7 15.2 10.9 15.2 11 15.2 11.1 15.3 11.2 15.3 11.3 15.4 11.4 15.5 11.6 15.6 11.6 15.7 11.7 15.8 11.8 15.9 11.9 16.1 12 16.2 12.1 16.4 12.2 16.5 12.3 16.7 12.4 16.9 12.5 17 12.5 17.3 12.6 17.5 12.7 17.8 12.8 18.1 12.9 18.4 13 18.7 13 19 13.1 19.3 13.1 19.5 13.1 19.7 13.1 20 13.1 20.2 13.1 20.4 13.1 20.7 13.1 21 13.1 21.2 13.1 21.3 13.1 21.5 13 21.7 13 22 13 22.2 12.9 22.4 12.9 22.6 12.8 22.9 12.8 23.1 12.7 23.2 12.7 23.4 12.6 23.6 12.5 23.8 12.4 24 12.3 24.1 12.2 24.2 12.1 24.4 12 24.6 11.8 24.7 11.7 24.8 11.6 24.9 11.4 25 11.3 25 11.2 25 11 25.1 11 25.1 10.8 25.1 10.7 25.1 10.6 25.1 10.5 25 10.4 25 10.2 24.9 10.1 24.8 10 24.8 9.9 24.7 9.8 24.6 9.7 24.5 9.6 24.4 9.5 24.3 9.4 24.1 9.3 24 9.2 23.9 9.1 23.7 9 23.5 8.9 23.3 8.9 23.1 8.8 22.8 8.7 22.6 8.6 22.3 8.5 22 8.5 21.7 8.4 21.4 8.4Z" fill="var(--bg)"/><path d="M10.4 10.2 10.2 10.2 9.9 10.2 9.7 10.1 9.4 10.1 9.1 10.1 8.9 10.2 8.7 10.2 8.5 10.3 8.3 10.3 8.2 10.4 8 10.5 7.8 10.6 7.7 10.7 7.6 10.8 7.5 10.9 7.5 11 7.5 11.2 7.5 11.3 7.5 11.4 7.6 11.5 7.7 11.6 7.9 11.7 8 11.8 8.2 11.8 8.4 11.9 8.7 12 9 12.1 9.3 12.1 9.5 12.1 9.8 12.1 10 12.1 10.2 12 10.5 12 10.7 11.9 10.8 11.9 11 11.8 11.1 11.8 11.3 11.7 11.5 11.5 11.7 11.4 11.8 11.2 11.8 11.1 11.8 11 11.7 10.8 11.6 10.7 11.5 10.7 11.4 10.6 11.2 10.5 11 10.4 10.7 10.3 10.6 10.3Z" fill="var(--bg)"/><path d="M21 6.9 20.8 6.9 20.5 6.9 20.2 6.9 20.1 6.9 19.9 6.9 19.6 7 19.4 7 19.2 7 19 7.1 18.8 7.1 18.6 7.2 18.3 7.2 18.1 7.3 17.9 7.4 17.8 7.4 17.6 7.5 17.4 7.6 17.3 7.7 17.1 7.8 17 7.9 16.8 8.1 16.7 8.2 16.6 8.4 16.5 8.5 16.5 8.6 16.4 8.8 16.4 8.9 16.4 9 16.4 9.1 16.5 9.2 16.5 9.4 16.6 9.5 16.7 9.6 16.7 9.7 16.8 9.8 16.9 9.8 17 9.9 17.2 10 17.3 10.1 17.5 10.2 17.7 10.3 17.9 10.4 18 10.5 18.2 10.5 18.5 10.6 18.8 10.7 19.1 10.7 19.3 10.8 19.5 10.8 19.8 10.8 20 10.9 20.3 10.9 20.6 10.9 20.8 10.9 21 10.8 21.2 10.8 21.4 10.8 21.6 10.8 21.9 10.7 22.1 10.7 22.2 10.6 22.5 10.6 22.7 10.5 22.9 10.4 23.1 10.4 23.2 10.3 23.4 10.2 23.6 10.1 23.7 9.9 23.9 9.8 24 9.7 24.1 9.5 24.2 9.4 24.3 9.2 24.3 9.1 24.3 9 24.3 8.9 24.3 8.7 24.3 8.6 24.2 8.5 24.2 8.4 24.1 8.3 24.1 8.2 24 8.1 23.9 8 23.7 7.9 23.6 7.8 23.5 7.7 23.4 7.6 23.2 7.5 23 7.4 22.8 7.4 22.6 7.3 22.3 7.2 22 7.1 21.7 7 21.4 7 21.1 7Z" fill="var(--bg)"/><path d="M21.5 5.6 21.3 5.6 21 5.5 20.7 5.5 20.5 5.5 20.2 5.5 20 5.5 19.8 5.5 19.6 5.6 19.4 5.6 19.1 5.7 18.9 5.7 18.7 5.8 18.6 5.8 18.4 5.9 18.2 6 18 6.1 17.8 6.3 17.7 6.4 17.6 6.6 17.5 6.7 17.4 6.9 17.4 7 17.4 7.1 17.4 7.2 17.5 7.3 17.5 7.4 17.6 7.5 17.7 7.7 17.8 7.8 17.9 7.9 18 7.9 18.1 8 18.3 8.1 18.5 8.2 18.7 8.3 19 8.4 19.3 8.5 19.6 8.5 19.9 8.6 20.1 8.6 20.4 8.6 20.7 8.6 20.9 8.6 21.1 8.6 21.3 8.5 21.5 8.5 21.7 8.5 22 8.4 22.2 8.3 22.3 8.3 22.5 8.2 22.7 8.1 22.9 8 23.1 7.9 23.2 7.8 23.3 7.7 23.3 7.6 23.4 7.5 23.5 7.3 23.5 7.2 23.5 7.1 23.5 7 23.5 6.8 23.5 6.7 23.4 6.6 23.3 6.5 23.3 6.4 23.1 6.3 23 6.2 22.9 6.1 22.8 6.1 22.6 6 22.4 5.9 22.2 5.8 22 5.7 21.8 5.7 21.5 5.6Z" fill="var(--bg)"/><path d="M20.8 4.2 20.6 4.2 20.3 4.2 20.1 4.2 19.8 4.2 19.6 4.3 19.4 4.3 19.3 4.4 19.1 4.5 18.9 4.6 18.8 4.6 18.7 4.8 18.6 4.9 18.5 5 18.5 5.1 18.5 5.2 18.5 5.3 18.5 5.5 18.6 5.6 18.7 5.7 18.8 5.8 18.9 5.9 19.1 6 19.3 6 19.5 6.1 19.7 6.2 20 6.2 20.2 6.3 20.4 6.3 20.7 6.3 21 6.2 21.2 6.2 21.4 6.2 21.6 6.1 21.8 6.1 21.9 6 22.1 5.9 22.3 5.8 22.4 5.7 22.5 5.5 22.6 5.4 22.6 5.3 22.6 5.1 22.6 5 22.5 4.9 22.4 4.8 22.3 4.7 22.2 4.6 22.1 4.5 22 4.5 21.7 4.4 21.5 4.3 21.2 4.2 20.9 4.2Z" fill="var(--accent)" stroke="var(--accent)"/></g></svg>',
    overview: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18 9 9l4 5 3-4 5 8z"/><path d="M3 21h18"/></svg>',
    explore: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="10" height="11" rx="2"/><rect x="15" y="3" width="6" height="6" rx="2"/><rect x="15" y="11" width="6" height="10" rx="2"/><rect x="3" y="16" width="10" height="5" rx="2"/></svg>',
    grew: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 17l6-6 4 4 8-8"/><path d="M15 7h6v6"/></svg>',
    cleanup: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16"/><path d="M9 7V4h6v3"/><path d="M6 7l1 13h10l1-13"/></svg>',
    scan: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v5h-5"/></svg>',
    gear: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H9a1.6 1.6 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V9a1.6 1.6 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1z"/></svg>',
    drive: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><rect x="3" y="6" width="18" height="12" rx="3"/><path d="M7 14h.01M11 14h6"/></svg>',
    clock: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>',
    flag: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M5 21V4h11l-1.5 4L16 12H7v9z"/></svg>',
    arrowUp: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M6 11l6-6 6 6"/></svg>',
    twins: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"><rect x="3" y="7" width="11" height="13" rx="2"/><rect x="10" y="3" width="11" height="13" rx="2" fill="currentColor" fill-opacity=".25"/></svg>',
    cloud: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 18a5 5 0 1 1 1-9.9A6 6 0 0 1 19.5 11 3.5 3.5 0 0 1 18 18z"/></svg>',
    route: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="6" r="2.5"/><path d="M8 16c4-1 2-6 6-8" stroke-dasharray="2 3"/></svg>',
    expand: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7"/></svg>',
    map: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M3 6l6-3 6 3 6-3v15l-6 3-6-3-6 3z"/><path d="M9 3v15M15 6v15"/></svg>',
    more: '<svg viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/></svg>',
    pie: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><path d="M12 3v9l6.5 6.2"/></svg>',
    blocks: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18M12 12h9M3 15h9"/></svg>',
    ring: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4"/><path d="M12 3v5M21 12h-5"/></svg>',
    treeIc: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M4 5h6M8 12h6M8 19h6M6 5v14M6 12h2M6 19h2"/></svg>',
    mountain: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M2 20 9 7l4 6 2-3 7 10z"/></svg>',
    info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.01"/></svg>',
    spark: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M12 3l2.2 6.8L21 12l-6.8 2.2L12 21l-2.2-6.8L3 12l6.8-2.2z"/></svg>',
    folder: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5H9l2 2h8.5A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"/></svg>',
    copy: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/></svg>',
    basket: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M3 9h18l-2 11H5z"/><path d="M8 9l4-6 4 6"/></svg>',
    search: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>',
    cloudDown: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 17a5 5 0 1 1 1-9.9A6 6 0 0 1 19.5 10 3.5 3.5 0 0 1 18 17"/><path d="M12 11v9M9 17l3 3 3-3"/></svg>',
    pin: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4h6l-1 6 3 3H7l3-3z"/><path d="M12 13v8"/></svg>',
    lake: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M3 8h18"/><path d="M6 8c1 6 3 9 6 9s5-3 6-9" stroke-dasharray="2 2.5"/></svg>',
    shield: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/></svg>',
  };
  Lustro.icons = I;

  Lustro.CATS = {
    video: "Video", images: "Images", audio: "Audio", docs: "Documents", archives: "Archives",
    installers: "Installers", dev: "Code & dev", apps: "Apps & games", system: "System", other: "Other",
  };

  // ---- Cloud providers and file states (same rules for every provider) ----
  // Text abbreviations rather than logos, so the design carries no third-party branding.
  Lustro.PROVIDERS = {
    onedrive: { label: "OneDrive", short: "OD" }, gdrive: { label: "Google Drive", short: "GD" },
    icloud: { label: "iCloud", short: "iC" }, dropbox: { label: "Dropbox", short: "DB" },
    box: { label: "Box", short: "BX" }, other: { label: "Other provider", short: "··" },
  };
  Lustro.CLOUD_STATES = {
    cloud: { label: "Cloud only", note: "Listed here, 0 bytes on this disk" },
    downloaded: { label: "Downloaded", note: "On this disk and counted as local · can be freed back to the cloud" },
    pinned: { label: "Always on this device", note: "Pinned: on this disk and counted as local" },
  };
  // Small marker: provider abbreviation + state icon. Cloud-only uses the hatched outline elsewhere.
  Lustro.cloudMark = function (n, cls) {
    if (!n.provider || !n.state) return "";
    const p = Lustro.PROVIDERS[n.provider], ic = { cloud: Lustro.icons.cloud, downloaded: Lustro.icons.cloudDown, pinned: Lustro.icons.pin }[n.state];
    return `<span class="cmark ${n.state} ${cls || ""}" title="${p.label} · ${Lustro.CLOUD_STATES[n.state].label}">${p.short}${ic}</span>`;
  };
  Lustro.cloudBadge = function (n) {
    if (!n.provider) return "";
    const p = Lustro.PROVIDERS[n.provider];
    return n.state ? `<span class="badge cloud">${p.label} · ${Lustro.CLOUD_STATES[n.state].label}</span>` : `<span class="badge cloud">${p.label}</span>`;
  };

  // ---- Theme (per-viewer convenience only) ----
  function storedTheme() { try { return localStorage.getItem("lustro-theme"); } catch (e) { return null; } }
  Lustro.setTheme = function (t) {
    try { localStorage.setItem("lustro-theme", t); } catch (e) {}
    applyTheme(t);
  };
  function applyTheme(t) {
    const resolved = t === "system" ? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark") : t || "dark";
    document.documentElement.setAttribute("data-theme", resolved);
    document.dispatchEvent(new CustomEvent("lustro-theme"));
  }
  Lustro.theme = () => storedTheme() || "dark";
  applyTheme(Lustro.theme());

  Lustro.pref = function (k, v) {
    try {
      if (v === undefined) return localStorage.getItem("lustro-" + k);
      localStorage.setItem("lustro-" + k, v);
    } catch (e) { return null; }
  };

  Lustro.css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  Lustro.catColor = (cat) => Lustro.css("--cat-" + cat);

  Lustro.fmt = function (gb, opts) {
    const sign = opts && opts.sign && gb > 0 ? "+" : gb < 0 ? "−" : "";
    const v = Math.abs(gb);
    if (v >= 1000) return sign + (v / 1000).toFixed(2) + " TB";
    if (v >= 100) return sign + v.toFixed(0) + " GB";
    if (v >= 1) return sign + v.toFixed(1) + " GB";
    return sign + (v * 1024).toFixed(0) + " MB";
  };

  Lustro.leaves = function (n, out) {
    out = out || [];
    if (n.children) n.children.forEach((c) => Lustro.leaves(c, out)); else out.push(n);
    return out;
  };
  Lustro.all = function (n, out) {
    out = out || [];
    out.push(n);
    if (n.children) n.children.forEach((c) => Lustro.all(c, out));
    return out;
  };
  Lustro.depth = (n) => { let d = 0; while (n.parent) { d++; n = n.parent; } return d; };
  Lustro.dominantCat = function (n) {
    if (!n.children) return n.cat;
    const m = {};
    Lustro.leaves(n).forEach((l) => (m[l.cat] = (m[l.cat] || 0) + l.size));
    return Object.keys(m).sort((a, b) => m[b] - m[a])[0] || "other";
  };
  Lustro.find = (path) => Lustro.all(Lustro.data.root).find((n) => n.path === path);
  Lustro.byId = (id) => Lustro.all(Lustro.data.root).find((n) => n.id === id);
  // Keyboard shortcut label in the platform's style: "Ctrl+K" or "⌘K".
  Lustro.kbd = (key) => (Lustro.P.mod === "⌘" ? "⌘" + key : Lustro.P.mod + "+" + key);

  // ---- Squarified treemap ----
  Lustro.squarify = function (nodes, x, y, w, h, value) {
    value = value || ((n) => n.size);
    const total = nodes.reduce((s, n) => s + value(n), 0);
    const out = [];
    if (!total || w <= 0 || h <= 0) return out;
    const scale = (w * h) / total;
    const items = nodes.map((n) => ({ n, a: value(n) * scale })).filter((i) => i.a > 0);
    let row = [];
    const worst = (r, side) => {
      const s = r.reduce((t, i) => t + i.a, 0);
      let max = 0, min = Infinity;
      r.forEach((i) => { max = Math.max(max, i.a); min = Math.min(min, i.a); });
      return Math.max((side * side * max) / (s * s), (s * s) / (side * side * min));
    };
    const layout = (r) => {
      const s = r.reduce((t, i) => t + i.a, 0);
      if (w >= h) {
        const cw = s / h; let cy = y;
        r.forEach((i) => { const ch = i.a / cw; out.push({ n: i.n, x, y: cy, w: cw, h: ch }); cy += ch; });
        x += cw; w -= cw;
      } else {
        const ch = s / w; let cx = x;
        r.forEach((i) => { const cw = i.a / ch; out.push({ n: i.n, x: cx, y, w: cw, h: ch }); cx += cw; });
        y += ch; h -= ch;
      }
    };
    while (items.length) {
      const side = Math.min(w, h);
      const next = items[0];
      if (!row.length || worst(row, side) >= worst(row.concat(next), side)) { row.push(items.shift()); }
      else { layout(row); row = []; }
    }
    if (row.length) layout(row);
    return out;
  };

  // ---- Tooltip ----
  let tipEl;
  Lustro.tip = function (html, ev) {
    if (!tipEl) { tipEl = document.createElement("div"); tipEl.className = "tip"; document.body.appendChild(tipEl); }
    if (!html) { tipEl.classList.remove("show"); return; }
    tipEl.innerHTML = html;
    const pad = 14, r = tipEl.getBoundingClientRect();
    let x = ev.clientX + pad, y = ev.clientY + pad;
    if (x + r.width > innerWidth - 8) x = ev.clientX - r.width - pad;
    if (y + r.height > innerHeight - 8) y = ev.clientY - r.height - pad;
    tipEl.style.left = x + "px"; tipEl.style.top = y + "px";
    tipEl.classList.add("show");
  };
  Lustro.nodeTip = function (n) {
    const cat = n.children ? Lustro.dominantCat(n) : n.cat;
    let h = `<b>${n.name}</b><span class="mono faint">${n.path}</span><br>`;
    h += `<span class="dot" style="background:${Lustro.catColor(cat)}"></span> ${Lustro.CATS[cat]} · <span class="mono">${Lustro.fmt(n.size)}</span>`;
    if (n.provider && n.state) h += `<br><span style="color:var(--cat-cloud)">${Lustro.PROVIDERS[n.provider].label} · ${Lustro.CLOUD_STATES[n.state].label}</span><br><span class="muted">${Lustro.CLOUD_STATES[n.state].note}</span>`;
    else if (n.provider) h += `<br><span style="color:var(--cat-cloud)">${Lustro.PROVIDERS[n.provider].label} sync folder · ${Lustro.fmt(n.onDisk)} on this disk</span>`;
    if (n.d) h += `<br><span class="${n.d > 0 ? "up" : "down"}">${Lustro.fmt(n.d, { sign: true })} since last scan</span>`;
    if (n.dup) h += `<br>Duplicate of <span class="mono">${n.dup}</span>`;
    if (n.sys) h += `<br><span class="muted">${n.sys}</span>`;
    return h;
  };

  // ---- Shell ----
  Lustro.shell = function (active, context) {
    const tabs = [
      ["index.html", "overview", "Overview", I.overview],
      ["explore.html", "explore", "Explore", I.explore],
      ["grew.html", "grew", "What grew", I.grew],
      ["cleanup.html", "cleanup", "Cleanup", I.cleanup],
    ];
    const P = Lustro.P;
    const top = document.createElement("header");
    top.className = "topbar chrome-" + P.id;
    // Window controls follow the platform: traffic lights top-left on macOS, controls top-right elsewhere.
    const lights = P.id === "mac" ? `<div class="lights"><i></i><i></i><i></i></div>` : "";
    const winCtl = P.id === "win" ? `<div class="winctl"><span>&#8212;</span><span>&#9633;</span><span>&#10005;</span></div>`
      : P.id === "linux" ? `<div class="winctl gnome"><span>&#10005;</span></div>` : "";
    top.innerHTML = `
      <div class="brand">${lights}${I.logo}<span>Lustro</span></div>
      <nav class="nav">${tabs.map(([href, key, label, ic]) =>
        `<a href="${href}" class="${key === active ? "active" : ""}">${ic}${label}</a>`).join("")}</nav>
      <div class="top-actions">
        <a class="btn primary" href="scan.html" title="${Lustro.kbd("R")}">${I.scan}Scan</a>
        <a class="icon-btn ${active === "settings" ? "active" : ""}" href="settings.html" title="Settings (${Lustro.kbd(",")})">${I.gear}</a>
        ${winCtl}
      </div>`;
    document.body.prepend(top);
    Lustro.platformBar();

    if (context !== false) {
      const d = Lustro.data.drive;
      const ctx = document.createElement("div");
      ctx.className = "context";
      ctx.innerHTML = `
        <div class="title">${I.drive}<span>${d.name}</span></div>
        <div class="sep"></div>
        <div class="meta">${I.clock}<span>Scanned ${Lustro.data.scannedAt}</span></div>
        <div class="meta"><span class="mono">${Lustro.fmt(d.used)}</span><span class="faint">of ${Lustro.fmt(d.capacity)} used</span></div>
        <div class="spacer"></div>
        <div class="drive-chip">${Lustro.data.drives.map((x, i) => `<button class="${i ? "" : "on"}" title="${x.name}">${x.short}</button>`).join("")}</div>`;
      document.querySelector(".page").prepend(ctx);
    }
  };

  // Prototype-only control (not product UI): switches sample paths, wording, shortcuts and window chrome.
  Lustro.platformBar = function () {
    const b = document.createElement("div");
    b.className = "proto-bar";
    b.innerHTML = `<span>Prototype platform</span>` + Object.values(Lustro.PLATFORMS).map((p) =>
      `<button data-p="${p.id}" class="${p.id === Lustro.P.id ? "on" : ""}">${p.label}</button>`).join("");
    b.onclick = (e) => {
      const btn = e.target.closest("button"); if (!btn) return;
      try { localStorage.setItem("lustro-platform", btn.dataset.p); } catch (err) {}
      location.reload();
    };
    document.body.appendChild(b);
  };

  Lustro.note = function (html) {
    const n = document.createElement("div");
    n.className = "note";
    n.innerHTML = `<button aria-label="Close">✕</button><b>Design note</b><br>${html}`;
    n.querySelector("button").onclick = () => n.remove();
    document.body.appendChild(n);
  };
})();

// A stand-in for a program that asks its terminal for the Kitty keyboard
// protocol — Claude Code does, with these flags — and the e2e's way of seeing
// what reaches such a program through the PTY (ConPTY, on Windows).
//
// It turns the protocol on (CSI > 5 u: disambiguate and report alternates),
// asks what the terminal says (CSI ? u), and writes every chunk it is read as
// hex on a line of its own, `sonda> 1b5b3f3575`. A plain q ends it, turning
// the protocol off again (CSI < u).
//
//   node sonda-teclado.mjs
process.stdin.setRawMode(true);
process.stdin.resume();
process.stdout.write("sonda: pronta\r\n\x1b[>5u\x1b[?u");
process.stdin.on("data", (chunk) => {
  process.stdout.write(`sonda> ${chunk.toString("hex")}\r\n`);
  if (chunk.length === 1 && chunk[0] === 0x71) {
    process.stdout.write("\x1b[<usonda: fim\r\n");
    process.exit(0);
  }
});

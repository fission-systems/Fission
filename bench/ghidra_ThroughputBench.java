// Ghidra's own PcodeEmulator on the same byte sequences fission-emulator and
// Unicorn were given. Same code, same instruction counts, same two-point
// method so translation/warm-up falls out of the difference.
import ghidra.app.script.GhidraScript;
import ghidra.pcode.emu.PcodeEmulator;
import ghidra.pcode.emu.PcodeThread;
import ghidra.program.model.address.Address;
import ghidra.program.model.lang.Language;
import ghidra.program.model.lang.LanguageID;
import ghidra.program.model.lang.RegisterValue;
import ghidra.program.model.lang.Register;
import java.math.BigInteger;

public class ThroughputBench extends GhidraScript {

    static final long CODE_BASE = 0x10000000L;
    static final long STACK_TOP = 0x20000000L;

    static final byte[] LOOP_CODE = { (byte)0x83,(byte)0xC0,0x01, (byte)0x83,(byte)0xE9,0x01,
                                      (byte)0xEB,(byte)0xF8 };
    static final byte[] MEM_CODE  = { 0x48,(byte)0x89,0x45,0x00, 0x48,(byte)0x8B,0x45,0x00,
                                      (byte)0x83,(byte)0xE9,0x01, (byte)0xEB,(byte)0xF3 };

    double run(Language lang, byte[] code, long count) throws Exception {
        PcodeEmulator emu = new PcodeEmulator(lang);
        Address base = lang.getDefaultSpace().getAddress(CODE_BASE);
        emu.getSharedState().setVar(base, code.length, true, code);

        PcodeThread<byte[]> thread = emu.newThread();
        thread.overrideCounter(base);
        Register rcx = lang.getRegister("RCX");
        Register rbp = lang.getRegister("RBP");
        Register rsp = lang.getRegister("RSP");
        Register rax = lang.getRegister("RAX");
        thread.overrideContextWithDefault();
        thread.getState().setVar(rcx, BigInteger.valueOf(count).toByteArray().length == 8
            ? BigInteger.valueOf(count).toByteArray() : pad(BigInteger.valueOf(count), 8));
        thread.getState().setVar(rbp, pad(BigInteger.valueOf(STACK_TOP - 0x1000), 8));
        thread.getState().setVar(rsp, pad(BigInteger.valueOf(STACK_TOP - 0x2000), 8));
        thread.getState().setVar(rax, pad(BigInteger.ZERO, 8));

        long t0 = System.nanoTime();
        for (long i = 0; i < count; i++) {
            thread.stepInstruction();
        }
        long t1 = System.nanoTime();
        return (t1 - t0) / 1e9;
    }

    // Ghidra's byte[] state is big-endian-agnostic raw bytes of the register
    // width; build them explicitly rather than relying on BigInteger's length.
    static byte[] pad(BigInteger v, int len) {
        byte[] out = new byte[len];
        byte[] raw = v.toByteArray();
        int n = Math.min(len, raw.length);
        System.arraycopy(raw, raw.length - n, out, len - n, n);
        return out;
    }

    @Override
    public void run() throws Exception {
        Language lang = getState().getTool() == null
            ? ghidra.program.util.DefaultLanguageService.getLanguageService()
                  .getLanguage(new LanguageID("x86:LE:64:default"))
            : currentProgram.getLanguage();

        String[] names = { "register loop", "memory loop" };
        byte[][] codes = { LOOP_CODE, MEM_CODE };
        long n1 = 200_000, n2 = 2_000_000;
        for (int i = 0; i < 2; i++) {
            double t1 = run(lang, codes[i], n1);
            double t2 = run(lang, codes[i], n2);
            println(String.format("%-16s %d in %.3fs, %d in %.3fs  ->  marginal %.2fM inst/s",
                names[i], n1, t1, n2, t2, (double)(n2 - n1) / (t2 - t1) / 1e6));
        }
    }
}

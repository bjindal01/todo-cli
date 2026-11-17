import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HexFormat;

/** Reads hex-encoded byte strings, one per line, and prints each decoded as Java decodes UTF-8, re-encoded as hex. */
public class DecodeUtf8 {
    public static void main(String[] args) throws Exception {
        HexFormat hex = HexFormat.of();
        StringBuilder out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String decoded = new String(hex.parseHex(line), StandardCharsets.UTF_8);
            out.append(hex.formatHex(decoded.getBytes(StandardCharsets.UTF_8))).append('\n');
        }
        System.out.print(out);
    }
}

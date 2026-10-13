package com.rustchat.bridge;

/**
 * RustChat yardimci modulu: giden mesajlari isler.
 * - "/komut arg ..." bicimindeki girisleri ayristirir
 * - normal mesajlar icin istatistik uretir (kelime, karakter, link, hashtag, emoji...)
 * - yarim komutlar icin kisa kullanim ipucu dondurur
 * Tum donusler Rust tarafinda serde_json ile ayrıştırılabilen JSON metnidir.
 */
public final class MessageProcessor {

    private MessageProcessor() {}

    public static String process(String text) {
        if (text == null) {
            text = "";
        }
        String trimmed = text.trim();
        if (trimmed.startsWith("/") && trimmed.length() > 1) {
            int sp = indexOfSpace(trimmed);
            String cmd = (sp < 0 ? trimmed.substring(1) : trimmed.substring(1, sp)).toLowerCase();
            String rest = sp < 0 ? "" : trimmed.substring(sp + 1).trim();
            StringBuilder args = new StringBuilder();
            boolean first = true;
            for (String tok : rest.split("\\s+")) {
                if (tok.isEmpty()) continue;
                if (!first) args.append(',');
                args.append('"').append(escape(tok)).append('"');
                first = false;
            }
            return "{\"kind\":\"command\",\"cmd\":\"" + escape(cmd)
                    + "\",\"args\":[" + args + "],\"rest\":\"" + escape(rest) + "\"}";
        }

        Stats s = Stats.of(text);
        return "{\"kind\":\"message\",\"text\":\"" + escape(text) + "\""
                + ",\"stats\":{\"words\":" + s.words
                + ",\"chars\":" + s.chars
                + ",\"emojis\":" + s.emojis
                + ",\"links\":[" + s.linksJson() + "]"
                + ",\"mentions\":[" + s.mentionsJson() + "]"
                + ",\"hashtags\":[" + s.hashtagsJson() + "]}}";
    }

    public static String hint(String text) {
        if (text == null || !text.startsWith("/") || text.length() < 2) return "";
        String head = text.substring(1);
        if (indexOfSpace(head) >= 0) head = head.substring(0, indexOfSpace(head));
        head = head.toLowerCase();
        switch (head) {
            case "join": return "/join <oda> - odaya katil";
            case "msg": return "/msg <kisi> <mesaj> - ozel mesaj";
            case "me": return "/me <eylem> - rol eylemi";
            case "nick": return "/nick <yeni_isim> - isim degistir";
            case "clear": return "/clear - konusmayi temizle";
            case "help": return "/help - komut listesi";
            case "quit": return "/quit - baglantiyi kes";
            case "j": return "/j <oda> - kisa /join";
            default: return "";
        }
    }

    private static int indexOfSpace(String s) {
        for (int i = 0; i < s.length(); i++) {
            if (Character.isWhitespace(s.charAt(i))) return i;
        }
        return -1;
    }

    static String escape(String s) {
        StringBuilder b = new StringBuilder(s.length() + 8);
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': b.append("\\\""); break;
                case '\\': b.append("\\\\"); break;
                case '\n': b.append("\\n"); break;
                case '\r': b.append("\\r"); break;
                case '\t': b.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        b.append(String.format("\\u%04x", (int) c));
                    } else {
                        b.append(c);
                    }
            }
        }
        return b.toString();
    }

    private static final class Stats {
        int words;
        int chars;
        int emojis;
        final StringBuilder links = new StringBuilder();
        final StringBuilder mentions = new StringBuilder();
        final StringBuilder hashtags = new StringBuilder();

        static Stats of(String text) {
            Stats s = new Stats();
            s.chars = text.length();
            s.words = countWords(text);
            s.emojis = countEmojis(text);
            scanTokens(text, s);
            return s;
        }

        private static int countWords(String t) {
            int n = 0;
            boolean in = false;
            for (int i = 0; i < t.length(); i++) {
                boolean w = !Character.isWhitespace(t.charAt(i));
                if (w && !in) n++;
                in = w;
            }
            return n;
        }

        private static int countEmojis(String t) {
            int n = 0;
            for (int i = 0; i < t.length(); ) {
                int cp = t.codePointAt(i);
                if (isEmoji(cp)) n++;
                i += Character.charCount(cp);
            }
            return n;
        }

        private static boolean isEmoji(int cp) {
            return (cp >= 0x1F300 && cp <= 0x1FAFF)
                    || (cp >= 0x1F000 && cp <= 0x1F2FF)
                    || (cp >= 0x2600 && cp <= 0x27BF)
                    || (cp >= 0x2B00 && cp <= 0x2BFF)
                    || (cp >= 0x1F1E6 && cp <= 0x1F1FF)
                    || cp == 0x2764 || cp == 0x2705 || cp == 0x274C
                    || cp == 0x2B50 || cp == 0x203C || cp == 0x2049;
        }

        private static void scanTokens(String t, Stats s) {
            int i = 0;
            while (i < t.length()) {
                if (t.startsWith("http://", i) || t.startsWith("https://", i) || t.startsWith("www.", i)) {
                    int j = i;
                    while (j < t.length() && !Character.isWhitespace(t.charAt(j))) j++;
                    add(s.links, t.substring(i, j));
                    i = j;
                } else if (t.charAt(i) == '@' && i + 1 < t.length() && isNameChar(t.charAt(i + 1))) {
                    int j = i + 1;
                    while (j < t.length() && isNameChar(t.charAt(j))) j++;
                    add(s.mentions, t.substring(i + 1, j));
                    i = j;
                } else if (t.charAt(i) == '#' && i + 1 < t.length() && isNameChar(t.charAt(i + 1))) {
                    int j = i + 1;
                    while (j < t.length() && isNameChar(t.charAt(j))) j++;
                    add(s.hashtags, t.substring(i + 1, j));
                    i = j;
                } else {
                    i++;
                }
            }
        }

        private static boolean isNameChar(char c) {
            return Character.isLetterOrDigit(c) || c == '_' || c == '-' || c == '.';
        }

        private static void add(StringBuilder list, String item) {
            if (list.length() > 0) list.append(',');
            list.append('"').append(escape(item)).append('"');
        }

        String linksJson() { return links.toString(); }
        String mentionsJson() { return mentions.toString(); }
        String hashtagsJson() { return hashtags.toString(); }
    }
}

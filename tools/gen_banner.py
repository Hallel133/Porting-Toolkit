"""Generates the PORTING TOOLKIT banner embedded in crates/ptk-cli/src/banner.rs.

The letters are an original 5-row pixel font drawn for this project. Each
pixel is two block characters wide, and a light-shade drop shadow falls one
row down and one column right, in the spirit of the FIGlet fonts on
patorjk.com's "Text to ASCII Art Generator".

    python3 tools/gen_banner.py      # prints the banner
"""

GLYPHS = {
    "P": ["###.", "#..#", "###.", "#...", "#..."],
    "O": [".##.", "#..#", "#..#", "#..#", ".##."],
    "R": ["###.", "#..#", "###.", "#.#.", "#..#"],
    "T": ["#####", "..#..", "..#..", "..#..", "..#.."],
    "I": ["###", ".#.", ".#.", ".#.", "###"],
    "N": ["#...#", "##..#", "#.#.#", "#..##", "#...#"],
    "G": [".###", "#...", "#.##", "#..#", ".###"],
    "K": ["#..#", "#.#.", "##..", "#.#.", "#..#"],
    "L": ["#...", "#...", "#...", "#...", "####"],
}

FULL, SHADOW = "█", "░"


def pixels(word: str) -> list[str]:
    rows = ["" for _ in range(5)]
    for i, ch in enumerate(word):
        for r in range(5):
            rows[r] += ("." if i else "") + GLYPHS[ch][r]
    return rows


def render(word: str) -> list[str]:
    px = pixels(word)
    height, width = len(px) + 1, len(px[0]) * 2 + 1
    grid = [[" "] * width for _ in range(height)]
    for r, row in enumerate(px):
        for c, cell in enumerate(row):
            if cell == "#":
                # Shadow first, one row down and one column right.
                for x in (2 * c + 1, 2 * c + 2):
                    if grid[r + 1][x] == " ":
                        grid[r + 1][x] = SHADOW
    for r, row in enumerate(px):
        for c, cell in enumerate(row):
            if cell == "#":
                grid[r][2 * c] = grid[r][2 * c + 1] = FULL
    return ["".join(line).rstrip() for line in grid]


if __name__ == "__main__":
    for word in ("PORTING", "TOOLKIT"):
        print("\n".join(render(word)))

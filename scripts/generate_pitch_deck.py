#!/usr/bin/env python3
"""
ClockLend Pitch Deck Generator
Generates a professional 16:9 widescreen PowerPoint presentation (.pptx)
for the Solana Mobile × RadiantsDAO CLOCK IN Hackathon.
"""

import os
import pptx
from pptx.util import Inches, Pt
from pptx.dml.color import RGBColor
from pptx.enum.text import PP_ALIGN, MSO_ANCHOR
from pptx.enum.shapes import MSO_SHAPE

# --- Presentation Colors ---
BG_DARK = RGBColor(12, 20, 53)       # #0C1435 Deep Midnight Navy
BG_ALT = RGBColor(15, 24, 48)        # #0F1830 Alternate Dark Navy
BG_CARD = RGBColor(24, 35, 64)       # #182340 Card Background
BG_CARD_LIGHT = RGBColor(30, 44, 78) # #1E2C4E Elevated Card
EMERALD = RGBColor(20, 241, 149)     # #14F195 Solana Mint / Emerald Neon
CYAN = RGBColor(0, 212, 255)         # #00D4FF Electric Cyan
GOLD = RGBColor(255, 176, 32)        # #FFB020 Amber Gold
CORAL = RGBColor(255, 93, 93)        # #FF5D5D Alert Crimson
TEXT_WHITE = RGBColor(248, 250, 252) # #F8FAFC Heading / Primary Text
TEXT_MUTED = RGBColor(148, 163, 184) # #94A3B8 Silver / Subtitle Text
TEXT_SUB = RGBColor(203, 213, 225)   # #CBD5E1 Body Text
BORDER_CARD = RGBColor(45, 65, 105)  # #2D4169 Card Border
BORDER_EMERALD = RGBColor(20, 241, 149) # #14F195 Active Border

FONT_HEADING = "Trebuchet MS"
FONT_BODY = "Arial"
FONT_MONO = "Consolas"

def create_deck(output_path="ClockLend_Pitch_Deck.pptx"):
    prs = pptx.Presentation()
    prs.slide_width = Inches(13.333)
    prs.slide_height = Inches(7.5)
    blank_layout = prs.slide_layouts[6]

    logo_path = os.path.abspath("mobile/assets/logo.png")
    seeker_img_path = os.path.abspath("/home/rootkit/.gemini/antigravity-cli/brain/426697ce-9ee8-450a-9e1d-d915b4a17b2a/seeker_screen.png")
    if not os.path.exists(seeker_img_path):
        seeker_img_path = os.path.abspath("mobile/assets/android-icon-foreground.png")

    def add_background(slide, alt=False):
        bg = slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, 0, 0, Inches(13.333), Inches(7.5))
        bg.fill.solid()
        bg.fill.fore_color.rgb = BG_ALT if alt else BG_DARK
        bg.line.fill.background()
        return bg

    def add_header(slide, badge_text, title_text, subtitle_text=None, title_accent_word=None):
        # Badge
        badge = slide.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(0.8), Inches(0.5), Inches(len(badge_text) * 0.11 + 0.5), Inches(0.38))
        badge.fill.solid()
        badge.fill.fore_color.rgb = RGBColor(16, 45, 60)
        badge.line.color.rgb = EMERALD
        badge.line.width = Pt(1)
        tf = badge.text_frame
        tf.word_wrap = False
        tf.vertical_anchor = MSO_ANCHOR.MIDDLE
        p = tf.paragraphs[0]
        p.text = badge_text.upper()
        p.font.size = Pt(10)
        p.font.bold = True
        p.font.color.rgb = EMERALD
        p.font.name = FONT_HEADING
        p.alignment = PP_ALIGN.CENTER

        # Title
        title_box = slide.shapes.add_textbox(Inches(0.8), Inches(0.95), Inches(11.7), Inches(0.9))
        tf_title = title_box.text_frame
        tf_title.word_wrap = True
        tf_title.margin_left = tf_title.margin_top = tf_title.margin_right = tf_title.margin_bottom = 0
        p_title = tf_title.paragraphs[0]
        p_title.font.size = Pt(28)
        p_title.font.bold = True
        p_title.font.name = FONT_HEADING

        if title_accent_word and title_accent_word in title_text:
            parts = title_text.split(title_accent_word)
            r1 = p_title.add_run()
            r1.text = parts[0]
            r1.font.color.rgb = TEXT_WHITE
            r2 = p_title.add_run()
            r2.text = title_accent_word
            r2.font.color.rgb = EMERALD
            if len(parts) > 1:
                r3 = p_title.add_run()
                r3.text = parts[1]
                r3.font.color.rgb = TEXT_WHITE
        else:
            r = p_title.add_run()
            r.text = title_text
            r.font.color.rgb = TEXT_WHITE

        # Subtitle
        if subtitle_text:
            p_sub = tf_title.add_paragraph()
            p_sub.text = subtitle_text
            p_sub.font.size = Pt(14)
            p_sub.font.color.rgb = TEXT_MUTED
            p_sub.font.name = FONT_BODY
            p_sub.space_before = Pt(4)

        # Footer branding
        footer_box = slide.shapes.add_textbox(Inches(0.8), Inches(7.1), Inches(11.7), Inches(0.3))
        tf_foot = footer_box.text_frame
        tf_foot.margin_left = tf_foot.margin_top = tf_foot.margin_right = tf_foot.margin_bottom = 0
        p_foot = tf_foot.paragraphs[0]
        p_foot.text = "ClockLend · Solana Seeker Micro-Lending & Social Pawns · CLOCK IN Hackathon 2026"
        p_foot.font.size = Pt(9.5)
        p_foot.font.color.rgb = RGBColor(90, 110, 145)
        p_foot.font.name = FONT_BODY

    def add_notes(slide, notes_text):
        notes_slide = slide.notes_slide
        tf = notes_slide.notes_text_frame
        tf.text = notes_text

    def add_card(slide, left, top, width, height, glow=False, fill_color=BG_CARD):
        card = slide.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(left), Inches(top), Inches(width), Inches(height))
        card.fill.solid()
        card.fill.fore_color.rgb = fill_color
        card.line.color.rgb = BORDER_EMERALD if glow else BORDER_CARD
        card.line.width = Pt(1.5 if glow else 1)
        return card

    # ==========================================
    # SLIDE 1: TITLE SLIDE
    # ==========================================
    s1 = prs.slides.add_slide(blank_layout)
    add_background(s1, alt=False)

    # Top Tagline Badge
    badge1 = s1.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(4.2), Inches(0.9), Inches(4.9), Inches(0.42))
    badge1.fill.solid()
    badge1.fill.fore_color.rgb = RGBColor(16, 45, 60)
    badge1.line.color.rgb = EMERALD
    badge1.line.width = Pt(1.2)
    tf1 = badge1.text_frame
    tf1.vertical_anchor = MSO_ANCHOR.MIDDLE
    p = tf1.paragraphs[0]
    p.text = "SOLANA MOBILE × RADIANTSDAO · CLOCK IN"
    p.font.size = Pt(11)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING
    p.alignment = PP_ALIGN.CENTER

    # Logo
    if os.path.exists(logo_path):
        s1.shapes.add_picture(logo_path, Inches(5.666), Inches(1.6), width=Inches(2.0), height=Inches(2.0))

    # Main Title
    tbox = s1.shapes.add_textbox(Inches(1.5), Inches(3.7), Inches(10.333), Inches(1.6))
    tf = tbox.text_frame
    tf.word_wrap = True
    p = tf.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r1 = p.add_run()
    r1.text = "Clock"
    r1.font.size = Pt(56)
    r1.font.bold = True
    r1.font.color.rgb = TEXT_WHITE
    r1.font.name = FONT_HEADING
    r2 = p.add_run()
    r2.text = "Lend"
    r2.font.size = Pt(56)
    r2.font.bold = True
    r2.font.color.rgb = EMERALD
    r2.font.name = FONT_HEADING

    p2 = tf.add_paragraph()
    p2.alignment = PP_ALIGN.CENTER
    p2.space_before = Pt(8)
    r_sub = p2.add_run()
    r_sub.text = "Social credit, on-chain. Micro-lending & pawns for the Seeker in your pocket."
    r_sub.font.size = Pt(18)
    r_sub.font.color.rgb = TEXT_MUTED
    r_sub.font.name = FONT_BODY

    # 3 Pills below title
    pills_data = [
        ("● LIVE ON SOLANA MAINNET", EMERALD, RGBColor(16, 45, 60)),
        ("SOLANA SEEKER NATIVE · MWA & SEED VAULT", CYAN, RGBColor(12, 40, 75)),
        ("OCTOBER 2026", GOLD, RGBColor(45, 35, 20))
    ]
    pill_widths = [2.9, 4.4, 2.0]
    total_w = sum(pill_widths) + 0.6
    start_x = (13.333 - total_w) / 2.0
    cur_x = start_x

    for idx, (p_text, col, bg_col) in enumerate(pills_data):
        pw = pill_widths[idx]
        pill = s1.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(cur_x), Inches(5.6), Inches(pw), Inches(0.48))
        pill.fill.solid()
        pill.fill.fore_color.rgb = bg_col
        pill.line.color.rgb = col
        pill.line.width = Pt(1)
        ptf = pill.text_frame
        ptf.vertical_anchor = MSO_ANCHOR.MIDDLE
        pp = ptf.paragraphs[0]
        pp.text = p_text
        pp.font.size = Pt(11)
        pp.font.bold = True
        pp.font.color.rgb = col
        pp.font.name = FONT_HEADING
        pp.alignment = PP_ALIGN.CENTER
        cur_x += pw + 0.3

    # Specs Footer
    foot = s1.shapes.add_textbox(Inches(1.5), Inches(6.5), Inches(10.333), Inches(0.5))
    ftf = foot.text_frame
    fp = ftf.paragraphs[0]
    fp.alignment = PP_ALIGN.CENTER
    fp.text = "Native Rust Program (282 KB, Zero-Anchor) · Pyth Pull Oracles · Slashable SKR Reputation Bonds"
    fp.font.size = Pt(11.5)
    fp.font.color.rgb = RGBColor(120, 140, 175)
    fp.font.name = FONT_MONO

    add_notes(s1, """PRESENTER NOTES (Slide 1):
"ClockLend. We took how people actually lend to each other — friends, circles, marketplaces — and put it on-chain, on the Seeker. It is live on Solana mainnet. In the next seven minutes: the problem, the product, the proof, and why this deserves both prizes."
Objection Prep:
- Program ID: HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3
- Cluster: Solana Mainnet-Beta (and Devnet)
- Hardware: Solana Seeker (Android 14)""")

    # ==========================================
    # SLIDE 2: THE PROBLEM
    # ==========================================
    s2 = prs.slides.add_slide(blank_layout)
    add_background(s2, alt=True)
    add_header(s2, "THE PROBLEM", "Lending is built for whales, not for people.", "Two broken worlds: Overcollateralized DeFi bots vs. risky informal community credit.", "not for people")

    # Left Card: DeFi's High Wall
    add_card(s2, 0.8, 1.9, 5.7, 4.0, glow=False)
    tb_left = s2.shapes.add_textbox(Inches(1.05), Inches(2.1), Inches(5.2), Inches(3.6))
    tf_l = tb_left.text_frame
    tf_l.word_wrap = True

    p = tf_l.paragraphs[0]
    p.text = "🏛️ DeFi's Whale Wall"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = CORAL
    p.font.name = FONT_HEADING

    bullets_defi = [
        ("150%+ Overcollateralization", "Locks out 99% of everyday mobile users who just need liquidity."),
        ("Ruthless Liquidation Bots", "Strike the microsecond oracle prices dip — zero mercy, zero recourse."),
        ("Reputation Means Nothing", "A 10-year perfect borrower is treated the same as an anonymous bot."),
        ("Desktop-First UX", "Complex institutional interfaces built for web traders, not mobile phones.")
    ]
    for b_title, b_desc in bullets_defi:
        p_b = tf_l.add_paragraph()
        p_b.space_before = Pt(10)
        r_t = p_b.add_run()
        r_t.text = "• " + b_title + ": "
        r_t.font.bold = True
        r_t.font.size = Pt(13)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_b.add_run()
        r_d.text = b_desc
        r_d.font.size = Pt(12)
        r_d.font.color.rgb = TEXT_SUB

    # Right Card: The Informal Economy
    add_card(s2, 6.833, 1.9, 5.7, 4.0, glow=False)
    tb_right = s2.shapes.add_textbox(Inches(7.083), Inches(2.1), Inches(5.2), Inches(3.6))
    tf_r = tb_right.text_frame
    tf_r.word_wrap = True

    p = tf_r.paragraphs[0]
    p.text = "🤝 The Informal Economy (Chit Funds / ROSCAs)"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = GOLD
    p.font.name = FONT_HEADING

    bullets_inf = [
        ("Hundreds of Billions Moved", "Massive global volume moving through peer circles and merchant pacts."),
        ("Purely Trust-Based", "Zero collateral security or legal enforcement — defaults break communities."),
        ("Unbanked Vulnerability", "Lenders have no recourse when borrowers vanish or experience hardship."),
        ("Zero Portable Credit History", "Decades of responsible borrowing never yield a verifiable credit rating.")
    ]
    for b_title, b_desc in bullets_inf:
        p_b = tf_r.add_paragraph()
        p_b.space_before = Pt(10)
        r_t = p_b.add_run()
        r_t.text = "• " + b_title + ": "
        r_t.font.bold = True
        r_t.font.size = Pt(13)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_b.add_run()
        r_d.text = b_desc
        r_d.font.size = Pt(12)
        r_d.font.color.rgb = TEXT_SUB

    # Bottom Takeaway Card
    takeaway = add_card(s2, 0.8, 6.1, 11.733, 0.82, glow=True, fill_color=RGBColor(16, 45, 60))
    tb_t = s2.shapes.add_textbox(Inches(1.0), Inches(6.15), Inches(11.333), Inches(0.7))
    tf_t = tb_t.text_frame
    p_t = tf_t.paragraphs[0]
    p_t.alignment = PP_ALIGN.CENTER
    r_t1 = p_t.add_run()
    r_t1.text = "The Core Gap: "
    r_t1.font.bold = True
    r_t1.font.size = Pt(15)
    r_t1.font.color.rgb = EMERALD
    r_t2 = p_t.add_run()
    r_t2.text = "Neither side gets both trust AND collateral security. ClockLend bridges this on the Seeker."
    r_t2.font.size = Pt(15)
    r_t2.font.color.rgb = TEXT_WHITE

    add_notes(s2, """PRESENTER NOTES (Slide 2):
"Two worlds: DeFi demands 150%+ collateral and liquidates you with bots. Informal credit — chit funds, ROSCAs, lending circles — runs hundreds of billions a year on pure trust, with zero security and zero records. Neither has both. That is the gap ClockLend fills."

Objection Prep:
If asked: "Why not just use Aave or Kamino?"
Answer: They serve institutional whale collateral with algorithmic liquidation bots. We serve mobile peer-to-peer social credit with on-chain collateral security and a 24-hour social grace period. We don't compete with pooled institutional lending; we unlock an entirely untouched consumer category.""")

    # ==========================================
    # SLIDE 3: THE SOLUTION
    # ==========================================
    s3 = prs.slides.add_slide(blank_layout)
    add_background(s3, alt=False)
    add_header(s3, "THE SOLUTION", "Marketplace-style P2P credit, with collateral on-chain.", "Three intuitive borrowing modes powered by a human-centric debt settlement engine.", "collateral on-chain")

    # 3 Mode Cards
    modes = [
        ("⚡ Express Borrow", "1-Tap Instant Liquidity", "Collateral valued in real-time via Pyth pull oracles. The smart router automatically selects the lowest-APR available lending desk.", CYAN),
        ("🏪 Desks & Circles", "Merchant Lending Capital", "Community leaders & merchants deploy on-chain escrowed desks with their own APR, LTV limits, and custom duration rules.", EMERALD),
        ("🃏 Pawn Deck", "1:1 Social Pawn Listings", "Direct borrower-to-lender listings. Stake SOL or SKR collateral directly. Peers fund each other transparently on-chain.", GOLD)
    ]
    for idx, (m_title, m_sub, m_desc, col) in enumerate(modes):
        cx = 0.8 + idx * 4.0
        add_card(s3, cx, 1.85, 3.733, 2.5, glow=False)
        tb = s3.shapes.add_textbox(Inches(cx + 0.2), Inches(1.95), Inches(3.333), Inches(2.3))
        tf = tb.text_frame
        tf.word_wrap = True
        p1 = tf.paragraphs[0]
        p1.text = m_title
        p1.font.size = Pt(18)
        p1.font.bold = True
        p1.font.color.rgb = col
        p1.font.name = FONT_HEADING

        p2 = tf.add_paragraph()
        p2.text = m_sub
        p2.font.size = Pt(12)
        p2.font.bold = True
        p2.font.color.rgb = TEXT_WHITE
        p2.space_before = Pt(4)

        p3 = tf.add_paragraph()
        p3.text = m_desc
        p3.font.size = Pt(11.5)
        p3.font.color.rgb = TEXT_SUB
        p3.space_before = Pt(8)

    # 2 Economics Cards (Middle)
    add_card(s3, 0.8, 4.5, 5.7, 1.45, glow=False)
    tb_eco1 = s3.shapes.add_textbox(Inches(1.0), Inches(4.55), Inches(5.3), Inches(1.3))
    tf_e1 = tb_eco1.text_frame
    tf_e1.word_wrap = True
    p = tf_e1.paragraphs[0]
    p.text = "💰 Lenders Earn Real Yield"
    p.font.size = Pt(15)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING
    p2 = tf_e1.add_paragraph()
    p2.text = "Earn fixed APR on every loan originated, plus a 5% liquidation bonus if a loan defaults. Your desk, your rules, your risk profile."
    p2.font.size = Pt(11.5)
    p2.font.color.rgb = TEXT_SUB
    p2.space_before = Pt(4)

    add_card(s3, 6.833, 4.5, 5.7, 1.45, glow=False)
    tb_eco2 = s3.shapes.add_textbox(Inches(7.033), Inches(4.55), Inches(5.3), Inches(1.3))
    tf_e2 = tb_eco2.text_frame
    tf_e2.word_wrap = True
    p = tf_e2.paragraphs[0]
    p.text = "📈 Borrowers Keep Staking Returns"
    p.font.size = Pt(15)
    p.font.bold = True
    p.font.color.rgb = CYAN
    p.font.name = FONT_HEADING
    p2 = tf_e2.add_paragraph()
    p2.text = "Locked SKR remains staked inside protocol escrow while securing the loan. Reputation points and staking rewards flow uninterrupted."
    p2.font.size = Pt(11.5)
    p2.font.color.rgb = TEXT_SUB
    p2.space_before = Pt(4)

    # 3 Stat Metrics (Bottom Row)
    metrics = [
        ("24h", "Social Grace Period", "Circle rescues default before bots can liquidate", GOLD),
        ("90%", "Max Borrow LTV", "Unlocked by holding a staked SKR reputation bond", EMERALD),
        ("50%", "APR Fee Discount", "Exclusive rate reduction for top-tier SKR stakers", CYAN)
    ]
    for idx, (m_val, m_label, m_sub, col) in enumerate(metrics):
        cx = 0.8 + idx * 4.0
        add_card(s3, cx, 6.1, 3.733, 0.85, glow=True, fill_color=RGBColor(16, 45, 60))
        tb = s3.shapes.add_textbox(Inches(cx + 0.15), Inches(6.12), Inches(3.433), Inches(0.8))
        tf = tb.text_frame
        p = tf.paragraphs[0]
        r1 = p.add_run()
        r1.text = m_val + "  "
        r1.font.size = Pt(22)
        r1.font.bold = True
        r1.font.color.rgb = col
        r1.font.name = FONT_HEADING
        r2 = p.add_run()
        r2.text = m_label
        r2.font.size = Pt(12)
        r2.font.bold = True
        r2.font.color.rgb = TEXT_WHITE
        p2 = tf.add_paragraph()
        p2.text = m_sub
        p2.font.size = Pt(9.5)
        p2.font.color.rgb = TEXT_MUTED

    add_notes(s3, """PRESENTER NOTES (Slide 3):
"Three marketplaces, one app. Express matches you to the cheapest desk in one tap. Desks let merchants and communities lend their own capital with their own rules. The pawn deck is pure 1:1 peer lending. And the settlement layer is human: a ticking countdown clock on every loan, and a 24-hour social grace window where your circle — not a bot — gets first rights to rescue."
Objection Prep:
- How does the 24h grace period work? After loan maturity, the borrower gets 24 hours to repay. During this time, friends or circle members can step in to repay and rescue the collateral. Liquidation only executes after grace expiry.""")

    # ==========================================
    # SLIDE 4: THE PRODUCT (LIVE ON SEEKER)
    # ==========================================
    s4 = prs.slides.add_slide(blank_layout)
    add_background(s4, alt=True)
    add_header(s4, "THE PRODUCT", "The deck ends. The app begins.", "Engineered natively for the Solana Seeker mobile experience.", "The app begins")

    # Phone Image (Left)
    if os.path.exists(seeker_img_path):
        s4.shapes.add_picture(seeker_img_path, Inches(1.0), Inches(1.8), height=Inches(5.0))

    # Right Content Cards
    # Hardware Integrations Card
    add_card(s4, 4.0, 1.8, 8.533, 2.35, glow=True)
    tb_hw = s4.shapes.add_textbox(Inches(4.2), Inches(1.9), Inches(8.133), Inches(2.15))
    tf_hw = tb_hw.text_frame
    tf_hw.word_wrap = True
    p = tf_hw.paragraphs[0]
    p.text = "📱 Running on Real Solana Seeker Hardware"
    p.font.size = Pt(18)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING

    hw_bullets = [
        ("Seed Vault Integration", "Hardware-isolated keys with instant biometric signing (no copy-pasting seed phrases)."),
        ("Mobile Wallet Adapter (MWA 2.0)", "Seamless connection to Seeker native wallets (Phantom, Solflare, Webcade)."),
        ("Ticking Countdown Settlement", "Live debt countdown timers visually remind borrowers before grace periods trigger."),
        ("NFC Lending Circles", "Physical Seeker tap-to-borrow pairing between merchants and local customers.")
    ]
    for b_title, b_desc in hw_bullets:
        p_b = tf_hw.add_paragraph()
        p_b.space_before = Pt(5)
        r_t = p_b.add_run()
        r_t.text = "• " + b_title + ": "
        r_t.font.bold = True
        r_t.font.size = Pt(11.5)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_b.add_run()
        r_d.text = b_desc
        r_d.font.size = Pt(11)
        r_d.font.color.rgb = TEXT_SUB

    # Storyboard Card
    add_card(s4, 4.0, 4.35, 8.533, 2.45, glow=False)
    tb_sb = s4.shapes.add_textbox(Inches(4.2), Inches(4.45), Inches(8.133), Inches(2.25))
    tf_sb = tb_sb.text_frame
    tf_sb.word_wrap = True
    p = tf_sb.paragraphs[0]
    p.text = "🎬 90-Second Demo Storyboard"
    p.font.size = Pt(18)
    p.font.bold = True
    p.font.color.rgb = CYAN
    p.font.name = FONT_HEADING

    steps = [
        ("0:00", "Seeker in Hand", "Open ClockLend, biometric Seed Vault unlock with 1 tap."),
        ("0:20", "1-Tap Express", "Deposit 0.5 SOL collateral; Pyth values at $75; borrow $50 USDC instantly."),
        ("0:45", "Countdown Clock", "Loan appears on dashboard with live settlement clock & interest ticker."),
        ("1:05", "1-Swipe Repay", "Borrower repays $51 USDC; escrow releases 0.5 SOL collateral back to wallet."),
        ("1:20", "SKR Reputation Stake", "Stake 1,000 SKR bond; instant level up to 90% LTV tier with 50% APR discount.")
    ]
    for ts, st_title, st_desc in steps:
        p_s = tf_sb.add_paragraph()
        p_s.space_before = Pt(4)
        r_ts = p_s.add_run()
        r_ts.text = ts + " · "
        r_ts.font.bold = True
        r_ts.font.size = Pt(11)
        r_ts.font.color.rgb = GOLD
        r_ts.font.name = FONT_MONO
        r_t = p_s.add_run()
        r_t.text = st_title + " — "
        r_t.font.bold = True
        r_t.font.size = Pt(11)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_s.add_run()
        r_d.text = st_desc
        r_d.font.size = Pt(10.5)
        r_d.font.color.rgb = TEXT_SUB

    add_notes(s4, """PRESENTER NOTES (Slide 4):
"This screenshot is the actual app running on a Solana Seeker — not an emulator. After this deck, the video shows the same flows in motion, and then I hand you the phone. If you want, we take a live borrow on mainnet right now."
Live test device on table:
- Model: Solana Seeker (Device ID: SM02G40619122247)
- App: ClockLend Android Release Build (PID 28297)""")

    # ==========================================
    # SLIDE 5: WHY BLOCKCHAIN · WHY SEEKER
    # ==========================================
    s5 = prs.slides.add_slide(blank_layout)
    add_background(s5, alt=False)
    add_header(s5, "WHY BLOCKCHAIN · WHY SEEKER", "Remove the chain, and the escrow is just a promise.", "The crypto necessity test: Why this fundamentally cannot exist on Web2 infrastructure.", "just a promise")

    # Column 1: Why Blockchain
    add_card(s5, 0.8, 1.85, 5.7, 4.9, glow=False)
    tb_c1 = s5.shapes.add_textbox(Inches(1.05), Inches(2.05), Inches(5.2), Inches(4.5))
    tf_c1 = tb_c1.text_frame
    tf_c1.word_wrap = True
    p = tf_c1.paragraphs[0]
    p.text = "⛓️ What the Blockchain Makes Possible"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING

    chain_items = [
        ("Atomic PDA Escrows", "Collateral is locked cryptographically in non-custodial Program Derived Addresses. Neither ClockLend nor lenders can steal user collateral."),
        ("Pyth Pull Oracles", "On-chain valuation without centralized keeper bots or admin price feeds. Prices are verified on-demand inside the transaction."),
        ("Permissionless Credit", "Anyone on earth with a phone and collateral can borrow instantly without KYC hurdles, credit checks, or bank approvals."),
        ("Sovereign Credit Identity", "Reputation and default history are recorded transparently on-chain, creating a portable credit score that follows the wallet.")
    ]
    for title, desc in chain_items:
        p_i = tf_c1.add_paragraph()
        p_i.space_before = Pt(7)
        r_t = p_i.add_run()
        r_t.text = "✔ " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(12)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(10.5)
        r_d.font.color.rgb = TEXT_SUB

    # Column 2: Why Seeker
    add_card(s5, 6.833, 1.85, 5.7, 4.9, glow=True)
    tb_c2 = s5.shapes.add_textbox(Inches(7.083), Inches(2.05), Inches(5.2), Inches(4.5))
    tf_c2 = tb_c2.text_frame
    tf_c2.word_wrap = True
    p = tf_c2.paragraphs[0]
    p.text = "📱 Why the Seeker Is the Essential Unlock"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = CYAN
    p.font.name = FONT_HEADING

    seeker_items = [
        ("Seed Vault Hardware Security", "Hardware-isolated key custody transforms a mobile phone into a tamper-proof financial terminal with 1-tap biometric authorizations."),
        ("Mobile-First Target Market", "The 1.4B unbanked population conducts 100% of their financial lives on mobile devices. Desktop DeFi completely misses them."),
        ("Human Social Grace Alerts", "Operating-system level push alerts notify borrowers and their circles hours before grace periods expire, enabling peer debt rescue."),
        ("NFC Proximity Lending", "Physical Seeker tap-to-borrow pairing brings community ROSCAs, merchant financing, and peer lending into the physical world.")
    ]
    for title, desc in seeker_items:
        p_i = tf_c2.add_paragraph()
        p_i.space_before = Pt(7)
        r_t = p_i.add_run()
        r_t.text = "✔ " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(12)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(10.5)
        r_d.font.color.rgb = TEXT_SUB

    add_notes(s5, """PRESENTER NOTES (Slide 5):
"The crypto necessity test: strip the chain and the escrow becomes a handshake — no atomic lock, no enforcement, no portable credit history. Strip the Seeker and the one-tap Seed Vault signing disappears, and so does the market: mobile-first users need a mobile-first lender."
Objection Prep:
"Why not a Web2 app like Venmo or Affirm?"
Answer: Web2 requires bank rails, central custody of collateral, and debt collection agencies. ClockLend is non-custodial and globally accessible to anyone with a Seeker phone.""")

    # ==========================================
    # SLIDE 6: PROOF · SECURITY
    # ==========================================
    s6 = prs.slides.add_slide(blank_layout)
    add_background(s6, alt=True)
    add_header(s6, "SECURITY & FORMAL VERIFICATION", "Six audit rounds. 81 tests. Zero shortcuts.", "Multi-pass security reviews with independent exploit hunts and formal invariant proofs.", "Zero shortcuts")

    # 4 Stat Cards Top Row
    stat_data = [
        ("6", "Independent Audit Passes", "EthelSec, invariant analysis & red-team exploit hunts", EMERALD),
        ("81 / 81", "Regression Tests Passing", "Including property-based fuzz tests for solvency invariants", CYAN),
        ("2", "Adversarial PoC Hunts", "All criticals reproduced as executable tests and proven closed", GOLD),
        ("0", "Centralized Pricing Keys", "Zero keeper dependencies; tamper-proof on-chain Pyth feeds", TEXT_WHITE)
    ]
    for idx, (val, title, sub, col) in enumerate(stat_data):
        cx = 0.8 + idx * 3.0
        add_card(s6, cx, 1.85, 2.733, 1.45, glow=False)
        tb = s6.shapes.add_textbox(Inches(cx + 0.15), Inches(1.9), Inches(2.433), Inches(1.35))
        tf = tb.text_frame
        p = tf.paragraphs[0]
        p.text = val
        p.font.size = Pt(28)
        p.font.bold = True
        p.font.color.rgb = col
        p.font.name = FONT_HEADING
        p2 = tf.add_paragraph()
        p2.text = title
        p2.font.size = Pt(11)
        p2.font.bold = True
        p2.font.color.rgb = TEXT_WHITE
        p3 = tf.add_paragraph()
        p3.text = sub
        p3.font.size = Pt(9)
        p3.font.color.rgb = TEXT_MUTED

    # 4 Vulnerability Remediation Cards
    vuln_data = [
        ("🔒 Type Confusion Elimination", "Enforced strict 8-byte discriminators (CLK_POOL, CLK_LOAN, CLK_PAWN) and fail-closed AccountKind dispatch across all instructions."),
        ("🛡️ Admin Race & Root Elimination", "Removed hardcoded admin roots; validated upgrade authority solely against live BPF UpgradeableLoader ProgramData (13..45 offset layout)."),
        ("⚡ Escrow Front-Run Defense", "Asserted program PDA ownership across all 5 token escrow sites (SKR escrow, collateral escrow, pool vault) to reject pre-created accounts."),
        ("🌊 SKR Collateral Liquidation Restoration", "Replaced mint classifier with role-based Treasury dispatch in ClaimDefault, ensuring 5% protocol margin and collateral transfers execute reliably.")
    ]
    for idx, (title, desc) in enumerate(vuln_data):
        row = idx // 2
        col = idx % 2
        cx = 0.8 + col * 6.0
        cy = 3.5 + row * 1.55
        add_card(s6, cx, cy, 5.733, 1.4, glow=False)
        tb = s6.shapes.add_textbox(Inches(cx + 0.2), Inches(cy + 0.1), Inches(5.333), Inches(1.2))
        tf = tb.text_frame
        tf.word_wrap = True
        p = tf.paragraphs[0]
        p.text = title
        p.font.size = Pt(13)
        p.font.bold = True
        p.font.color.rgb = EMERALD
        p.font.name = FONT_HEADING
        p2 = tf.add_paragraph()
        p2.text = desc
        p2.font.size = Pt(11)
        p2.font.color.rgb = TEXT_SUB
        p2.space_before = Pt(3)

    # Bottom Hash Verified Banner
    banner = add_card(s6, 0.8, 6.25, 11.733, 0.65, glow=True, fill_color=RGBColor(16, 45, 60))
    tb_b = s6.shapes.add_textbox(Inches(1.0), Inches(6.3), Inches(11.333), Inches(0.55))
    tf_b = tb_b.text_frame
    p = tf_b.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r1 = p.add_run()
    r1.text = "Deployed Program: "
    r1.font.bold = True
    r1.font.size = Pt(13)
    r1.font.color.rgb = CYAN
    r2 = p.add_run()
    r2.text = "HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3  ·  Hash-Verified Live on Mainnet"
    r2.font.size = Pt(12)
    r2.font.color.rgb = TEXT_WHITE
    r2.font.name = FONT_MONO

    add_notes(s6, """PRESENTER NOTES (Slide 6):
"Two of this panel's judges are security researchers, so let me be precise: the program went through six independent audit passes, including PoC-driven exploit hunts and a property-based fuzz suite that checks solvency and escrow invariants after every simulated step. Everything found was fixed. The deployed binary is hash-verified on-chain."
Key Remediation Highlights:
- F1 SKR liquidation: Proven with new end-to-end integration tests.
- F3/F4 ProgramData verification: Eliminates any centralized admin backdoor.
- 81 automated tests running clean with 100% pass rate.""")

    # ==========================================
    # SLIDE 7: SHIPPED & TRACTION
    # ==========================================
    s7 = prs.slides.add_slide(blank_layout)
    add_background(s7, alt=False)
    add_header(s7, "DELIVERED & SHIPPED", "Shipped, not slides.", "Production-ready code, live mainnet contracts, and a complete publishing pipeline.", "not slides")

    # 3 Pillars
    pillars = [
        ("🚀 Solana Mainnet", "Deployed & Verified", [
            "282 KB optimized native Rust ELF",
            "Zero Anchor bloat, sub-millisecond execution",
            "Treasury PDA initialized and funded",
            "Genesis Seeker Lending Desk seeded live",
            "Pyth pull oracles wired for SOL & SKR"
        ], EMERALD),
        ("📱 Seeker Native App", "Installed on Hardware", [
            "Running on physical Seeker hardware",
            "MWA 2.0 & Seed Vault biometric integration",
            "Real-time countdown settlement clocks",
            "APK compiled and ready for dApp Store",
            "Full fail-closed biometric app locking"
        ], CYAN),
        ("⚙️ Autonomous Pipeline", "Zero Keeper Dependency", [
            "Mainnet deployment fully automated",
            "Auto-closing deploy buffer (rent refunded)",
            "CoinGecko backup price keeper script",
            "Self-healing oracle pull updates on borrow",
            "No centralized backend database required"
        ], GOLD)
    ]
    for idx, (p_title, p_sub, bullets, col) in enumerate(pillars):
        cx = 0.8 + idx * 4.0
        add_card(s7, cx, 1.85, 3.733, 4.0, glow=False)
        tb = s7.shapes.add_textbox(Inches(cx + 0.2), Inches(2.0), Inches(3.333), Inches(3.7))
        tf = tb.text_frame
        tf.word_wrap = True
        p = tf.paragraphs[0]
        p.text = p_title
        p.font.size = Pt(18)
        p.font.bold = True
        p.font.color.rgb = col
        p.font.name = FONT_HEADING

        p_subt = tf.add_paragraph()
        p_subt.text = p_sub
        p_subt.font.size = Pt(11)
        p_subt.font.bold = True
        p_subt.font.color.rgb = TEXT_WHITE
        p_subt.space_before = Pt(2)

        for b in bullets:
            pb = tf.add_paragraph()
            pb.space_before = Pt(8)
            r = pb.add_run()
            r.text = "• " + b
            r.font.size = Pt(11)
            r.font.color.rgb = TEXT_SUB

    # Honesty Box
    add_card(s7, 0.8, 6.05, 11.733, 0.85, glow=True, fill_color=RGBColor(16, 45, 60))
    tb_h = s7.shapes.add_textbox(Inches(1.0), Inches(6.1), Inches(11.333), Inches(0.75))
    tf_h = tb_h.text_frame
    tf_h.word_wrap = True
    p = tf_h.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r1 = p.add_run()
    r1.text = "Honest Traction: "
    r1.font.bold = True
    r1.font.size = Pt(13)
    r1.font.color.rgb = GOLD
    r2 = p.add_run()
    r2.text = "What we did NOT fabricate: fake DAU or wash-borrowed volume. In lending, an unpatched bug costs users real collateral. We spent the hackathon mathematically proving the settlement layer."
    r2.font.size = Pt(12)
    r2.font.color.rgb = TEXT_WHITE

    add_notes(s7, """PRESENTER NOTES (Slide 7):
"Honest traction: we are a hackathon team, so the traction is engineering depth, not DAU. The program is live on mainnet, the app runs on the Seeker, and the publishing path into the dApp Store is next."
Objection Prep:
If asked: "You have no users yet?"
Answer: "Correct, and deliberate. In lending, a smart contract bug costs real users their savings. We spent the hackathon window executing 6 audit passes and hardening the core. Real user acquisition starts the day we land in the Solana dApp Store.""")

    # ==========================================
    # SLIDE 8: THE $10,000 SKR INTEGRATION
    # ==========================================
    s8 = prs.slides.add_slide(blank_layout)
    add_background(s8, alt=True)
    add_header(s8, "THE $10,000 SKR INTEGRATION TRACK", "SKR isn't bolted on. It's the engine.", "Real economic rights, tiered fee discounts, slashable reputation bonds, and Pyth oracles.", "It's the engine")

    skr_cards = [
        ("🏆 Staking Tier Rights", "Direct Economic Utility", [
            "100 SKR Staked: Unlocks 25% APR fee discount across all lending desks.",
            "1,000 SKR Staked: Unlocks 50% APR fee discount + 90% LTV borrow tier.",
            "Continuous Yield: Staked SKR continues generating rewards while locked in loan escrow.",
            "Non-Custodial Exit: Unstake anytime from the mobile UI with zero lockup penalty when loans are cleared."
        ], EMERALD),
        ("🛡️ Slashed Reputation Bond", "Default Deterrence", [
            "Borrower stakes SKR as a skin-in-the-game credit bond.",
            "If loan defaults and 24h grace expires, the bond is slashed on-chain.",
            "Slashed SKR compensates the desk lender, protecting pool solvency.",
            "Clean borrowers build permanent on-chain credit scores."
        ], GOLD),
        ("🔮 Pyth SKR/USD Oracle", "First Protocol in Lending", [
            "First lending protocol to integrate Pyth's official SKR/USD price feed.",
            "Sub-second market valuation verified on-chain per transaction.",
            "Zero admin override, zero centralized keeper cron jobs.",
            "SKR accepted as primary collateral alongside SOL."
        ], CYAN)
    ]
    for idx, (title, sub, bullets, col) in enumerate(skr_cards):
        cx = 0.8 + idx * 4.0
        add_card(s8, cx, 1.85, 3.733, 4.95, glow=(idx == 0))
        tb = s8.shapes.add_textbox(Inches(cx + 0.2), Inches(2.05), Inches(3.333), Inches(4.55))
        tf = tb.text_frame
        tf.word_wrap = True
        p = tf.paragraphs[0]
        p.text = title
        p.font.size = Pt(18)
        p.font.bold = True
        p.font.color.rgb = col
        p.font.name = FONT_HEADING

        p_s = tf.add_paragraph()
        p_s.text = sub
        p_s.font.size = Pt(11)
        p_s.font.bold = True
        p_s.font.color.rgb = TEXT_WHITE
        p_s.space_before = Pt(2)

        for b in bullets:
            pb = tf.add_paragraph()
            pb.space_before = Pt(10)
            r = pb.add_run()
            r.text = "• " + b
            r.font.size = Pt(11.5)
            r.font.color.rgb = TEXT_SUB

    add_notes(s8, """PRESENTER NOTES (Slide 8):
"For the SKR track: SKR in ClockLend is a staking product with real economic rights — tiered APR discounts, a slashable default bond, and a collateral class. And we price it from Pyth's SKR/USD feed — the market's price, verified on-chain, with no centralized keeper. That is staking, rewards, and access in one product."
Key Takeaway for Judges:
- ClockLend directly drives structural staking demand for SKR.
- Borrowers need SKR to access 90% LTV and fee discounts.
- Merchants stake SKR to earn trusted merchant badges.""")

    # ==========================================
    # SLIDE 9: WHY THIS WINS FOR THE ECOSYSTEM
    # ==========================================
    s9 = prs.slides.add_slide(blank_layout)
    add_background(s9, alt=False)
    add_header(s9, "ECOSYSTEM IMPACT", "A lending layer for the Seeker social graph.", "Creating structural demand for SKR while delivering the flagship consumer lending dApp.", "Seeker social graph")

    # Left: For SKR
    add_card(s9, 0.8, 1.85, 5.7, 4.9, glow=False)
    tb_skr = s9.shapes.add_textbox(Inches(1.05), Inches(2.05), Inches(5.2), Inches(4.5))
    tf_skr = tb_skr.text_frame
    tf_skr.word_wrap = True
    p = tf_skr.paragraphs[0]
    p.text = "💎 Value Created for the SKR Token"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING

    skr_eco = [
        ("Structural Staking Sink", "Every merchant opening a lending desk and every borrower seeking discounted rates locks SKR in protocol escrow contracts."),
        ("Collateral Float Removal", "SKR locked as micro-loan collateral takes circulating supply off the market during active borrowing periods."),
        ("First Genuine DeFi Utility", "Transforms SKR from a speculative meme or governance token into a productive financial asset with cash-flow utility."),
        ("Counter-Cyclical Durability", "Micro-credit demand remains high regardless of crypto bull or bear cycles, sustaining continuous protocol volume.")
    ]
    for title, desc in skr_eco:
        p_i = tf_skr.add_paragraph()
        p_i.space_before = Pt(12)
        r_t = p_i.add_run()
        r_t.text = "✔ " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(13)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(11.5)
        r_d.font.color.rgb = TEXT_SUB

    # Right: For Seeker
    add_card(s9, 6.833, 1.85, 5.7, 4.9, glow=True)
    tb_sk = s9.shapes.add_textbox(Inches(7.083), Inches(2.05), Inches(5.2), Inches(4.5))
    tf_sk = tb_sk.text_frame
    tf_sk.word_wrap = True
    p = tf_sk.paragraphs[0]
    p.text = "📱 Value Created for the Solana Seeker"
    p.font.size = Pt(20)
    p.font.bold = True
    p.font.color.rgb = CYAN
    p.font.name = FONT_HEADING

    seeker_eco = [
        ("Flagship Financial Primitive", "A consumer-grade, everyday mobile dApp proving the consumer power of the Solana Mobile Stack and Seed Vault."),
        ("Portable On-Chain Credit Identity", "Creates the first decentralized credit score that follows Seeker wallet holders across future Web3 applications."),
        ("Organic Grassroots Distribution", "NFC circle lending and community desks encourage peer-to-peer Seeker hardware recommendations."),
        ("TARDIS Social Synergy", "Seamless integration into Seeker social feeds for viral peer loans, debt gifts, and social circle rescue alerts.")
    ]
    for title, desc in seeker_eco:
        p_i = tf_sk.add_paragraph()
        p_i.space_before = Pt(12)
        r_t = p_i.add_run()
        r_t.text = "✔ " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(13)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(11.5)
        r_d.font.color.rgb = TEXT_SUB

    add_notes(s9, """PRESENTER NOTES (Slide 9):
"The ecosystem case: ClockLend turns SKR from a holding into a working asset — staked for discounts, locked as collateral, priced by Pyth. And it gives every Seeker wallet a portable credit history — the missing primitive for the social graph Solana Mobile is building."
Key Ecosystem Takeaway:
- Seeker needs real utility dApps that can't exist on Apple/Google App Stores.
- ClockLend leverages Seed Vault, MWA, and NFC hardware features.""")

    # ==========================================
    # SLIDE 10: THE ASK & COMMITMENTS
    # ==========================================
    s10 = prs.slides.add_slide(blank_layout)
    add_background(s10, alt=True)
    add_header(s10, "THE ASK & COMMITMENTS", "Two prizes. One launch.", "A clear, actionable plan to take ClockLend from hackathon winner to Seeker essential.", "One launch")

    # Left: What We Ask
    add_card(s10, 0.8, 1.85, 5.7, 4.9, glow=False)
    tb_ask = s10.shapes.add_textbox(Inches(1.05), Inches(2.1), Inches(5.2), Inches(4.4))
    tf_ask = tb_ask.text_frame
    tf_ask.word_wrap = True
    p = tf_ask.paragraphs[0]
    p.text = "🎯 What We Are Asking For"
    p.font.size = Pt(22)
    p.font.bold = True
    p.font.color.rgb = GOLD
    p.font.name = FONT_HEADING

    ask_items = [
        ("CLOCK IN Grand Prize", "Recognition as the premier consumer mobile DeFi application built for the Solana Seeker ecosystem."),
        ("The $10,000 SKR Integration Prize", "For delivering the most comprehensive, economically robust SKR staking and oracle integration in the hackathon."),
        ("Solana Mobile Co-Marketing", "Support for featured placement on the Solana dApp Store home feed upon official device ship date.")
    ]
    for title, desc in ask_items:
        p_i = tf_ask.add_paragraph()
        p_i.space_before = Pt(10)
        r_t = p_i.add_run()
        r_t.text = "🏆 " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(13)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(11)
        r_d.font.color.rgb = TEXT_SUB

    # Right: What We Deliver
    add_card(s10, 6.833, 1.85, 5.7, 4.9, glow=True)
    tb_del = s10.shapes.add_textbox(Inches(7.083), Inches(2.1), Inches(5.2), Inches(4.4))
    tf_del = tb_del.text_frame
    tf_del.word_wrap = True
    p = tf_del.paragraphs[0]
    p.text = "🚀 What We Commit to Deliver"
    p.font.size = Pt(22)
    p.font.bold = True
    p.font.color.rgb = EMERALD
    p.font.name = FONT_HEADING

    deliver_items = [
        ("Solana dApp Store Launch", "Official submission and release package for all 150,000+ Solana Seeker pre-order customers."),
        ("Mainnet Liquidity Seeding", "Deploy dedicated capital to seed initial community lending desks for low-fee micro-borrowing."),
        ("First 1,000 Verified Borrowers", "Targeted user onboarding campaign starting with verified Seeker Genesis Token holders."),
        ("TARDIS Social Feed Integration", "Enable native social micro-loans and peer debt gifting directly inside Seeker social feeds.")
    ]
    for title, desc in deliver_items:
        p_i = tf_del.add_paragraph()
        p_i.space_before = Pt(8)
        r_t = p_i.add_run()
        r_t.text = "⚡ " + title + "\n"
        r_t.font.bold = True
        r_t.font.size = Pt(12.5)
        r_t.font.color.rgb = TEXT_WHITE
        r_d = p_i.add_run()
        r_d.text = desc
        r_d.font.size = Pt(11)
        r_d.font.color.rgb = TEXT_SUB

    add_notes(s10, """PRESENTER NOTES (Slide 10):
"The ask is specific: a grand prize and the SKR integration prize. What you get in return is equally specific: a published Seeker app, seeded mainnet desks, and a borrower community — the lending layer for the social graph Solana Mobile is building."
Key Closing Points:
- Ready to ship on day one of Seeker retail deliveries.
- Full mainnet deployment already verified.""")

    # ==========================================
    # SLIDE 11: CLOSING & LIVE DEMO
    # ==========================================
    s11 = prs.slides.add_slide(blank_layout)
    add_background(s11, alt=False)

    # Logo
    if os.path.exists(logo_path):
        s11.shapes.add_picture(logo_path, Inches(5.666), Inches(1.0), width=Inches(2.0), height=Inches(2.0))

    # Title & Tagline
    tbox_c = s11.shapes.add_textbox(Inches(1.5), Inches(3.1), Inches(10.333), Inches(1.3))
    tf_c = tbox_c.text_frame
    tf_c.word_wrap = True
    p = tf_c.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r1 = p.add_run()
    r1.text = "Clock"
    r1.font.size = Pt(48)
    r1.font.bold = True
    r1.font.color.rgb = TEXT_WHITE
    r1.font.name = FONT_HEADING
    r2 = p.add_run()
    r2.text = "Lend"
    r2.font.size = Pt(48)
    r2.font.bold = True
    r2.font.color.rgb = EMERALD
    r2.font.name = FONT_HEADING

    p2 = tf_c.add_paragraph()
    p2.alignment = PP_ALIGN.CENTER
    p2.space_before = Pt(6)
    r_sub = p2.add_run()
    r_sub.text = "Lend on time. Social credit, on-chain."
    r_sub.font.size = Pt(20)
    r_sub.font.bold = True
    r_sub.font.color.rgb = CYAN
    r_sub.font.name = FONT_HEADING

    # Verification Info Card
    add_card(s11, 2.0, 4.4, 9.333, 1.6, glow=True, fill_color=RGBColor(16, 45, 60))
    tb_info = s11.shapes.add_textbox(Inches(2.2), Inches(4.45), Inches(8.933), Inches(1.5))
    tf_i = tb_info.text_frame
    tf_i.word_wrap = True

    p = tf_i.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r = p.add_run()
    r.text = "MAINNET PROGRAM ID: "
    r.font.bold = True
    r.font.size = Pt(12)
    r.font.color.rgb = EMERALD
    r.font.name = FONT_HEADING
    r_id = p.add_run()
    r_id.text = "HAjGxuih14imCMaWvCnJQ3nSdWmS8PQKzp74gyAgjsH3"
    r_id.font.size = Pt(12)
    r_id.font.color.rgb = TEXT_WHITE
    r_id.font.name = FONT_MONO

    p2 = tf_i.add_paragraph()
    p2.alignment = PP_ALIGN.CENTER
    p2.space_before = Pt(6)
    r = p2.add_run()
    r.text = "OPEN SOURCE REPOSITORY: "
    r.font.bold = True
    r.font.size = Pt(12)
    r.font.color.rgb = CYAN
    r.font.name = FONT_HEADING
    r_repo = p2.add_run()
    r_repo.text = "github.com/luckysitara/Clock-It"
    r_repo.font.size = Pt(12)
    r_repo.font.color.rgb = TEXT_WHITE
    r_repo.font.name = FONT_MONO

    p3 = tf_i.add_paragraph()
    p3.alignment = PP_ALIGN.CENTER
    p3.space_before = Pt(8)
    r_tech = p3.add_run()
    r_tech.text = "Built for Solana Seeker · Mobile Wallet Adapter 2.0 · Seed Vault · Pyth Pull Oracles · SKR Staking"
    r_tech.font.size = Pt(11)
    r_tech.font.color.rgb = TEXT_SUB

    # Device Invite Bottom Box
    inv = s11.shapes.add_textbox(Inches(1.5), Inches(6.25), Inches(10.333), Inches(0.5))
    tf_inv = inv.text_frame
    p_inv = tf_inv.paragraphs[0]
    p_inv.alignment = PP_ALIGN.CENTER
    r_inv = p_inv.add_run()
    r_inv.text = "👉 The Seeker phone is on the table — let's execute a live mainnet micro-loan right now."
    r_inv.font.size = Pt(14)
    r_inv.font.bold = True
    r_inv.font.color.rgb = GOLD

    add_notes(s11, """PRESENTER NOTES (Slide 11):
"Thank you. The repo is public — the program, the audits, the fuzz suite, the app. And the phone is on the table: let's take a live borrow on mainnet."
Demo Checklist:
1. Show Seeker home screen with ClockLend icon.
2. Open ClockLend -> instant biometric Seed Vault unlock.
3. Show live Genesis Desk with Pyth price updates.
4. Execute instant micro-loan.""")

    # Save
    prs.save(output_path)
    print(f"✅ Presentation successfully generated and saved to: {output_path}")

if __name__ == "__main__":
    create_deck()

describe('Search and Reports (Feature 11)', () => {
  before(async () => {
    await browser.pause(1500);
  });

  // ── Step 2: Search UI ──────────────────────────────────────────────────────

  describe('Search on /transactions', () => {
    before(async () => {
      const link = await browser.$('a[href="/transactions"]');
      await link.click();
      await browser.pause(500);
    });

    it('should render the search input above the transaction list', async () => {
      const input = await browser.$('input[type="search"]');
      await expect(input).toExist();
    });

    it('should show full transaction list when search is empty', async () => {
      const input = await browser.$('input[type="search"]');
      const val = await input.getValue();
      expect(val).toBe('');
      // Table or empty state is visible (no results for "" message)
      const noResults = await browser.$('.no-results');
      await expect(noResults).not.toExist();
    });

    it('should show no-results message when query matches nothing', async () => {
      const input = await browser.$('input[type="search"]');
      await input.setValue('xyzzy_no_match_ever_12345');
      await browser.pause(500);
      const noResults = await browser.$('.no-results');
      await expect(noResults).toExist();
    });

    it('should clear results when query is erased', async () => {
      const input = await browser.$('input[type="search"]');
      await input.clearValue();
      await browser.pause(500);
      const noResults = await browser.$('.no-results');
      await expect(noResults).not.toExist();
    });
  });

  // ── Step 4: Reports route ──────────────────────────────────────────────────

  describe('Reports page', () => {
    before(async () => {
      const link = await browser.$('a[href="/reports"]');
      await link.click();
      await browser.pause(800);
    });

    it('should have a Reports link in the sidebar', async () => {
      const link = await browser.$('a[href="/reports"]');
      await expect(link).toExist();
    });

    it('should navigate to /reports and render the page heading', async () => {
      const url = await browser.execute(() => window.location.pathname);
      expect(url).toBe('/reports');

      const heading = await browser.$('.heading');
      await expect(heading).toExist();
      const text = await heading.getText();
      expect(text).toBe('Reports');
    });

    it('should render the Spending by Category section title', async () => {
      const sections = await browser.$$('.section-title');
      expect(sections.length).toBeGreaterThanOrEqual(1);
      const first = await sections[0].getText();
      expect(first.toUpperCase()).toContain('SPENDING BY CATEGORY');
    });

    it('should render the Monthly Trend section title', async () => {
      const sections = await browser.$$('.section-title');
      expect(sections.length).toBeGreaterThanOrEqual(2);
      const second = await sections[1].getText();
      expect(second.toUpperCase()).toContain('MONTHLY TREND');
    });

    it('should render the month picker with 13 options', async () => {
      const picker = await browser.$('.month-picker');
      await expect(picker).toExist();
      const options = await picker.$$('option');
      expect(options.length).toBe(13);
    });

    it('should default the month picker to the current month', async () => {
      const picker = await browser.$('.month-picker');
      const value = await picker.getValue();
      const now = new Date();
      const expected = `${now.getFullYear().toString().padStart(4, '0')}-${(now.getMonth() + 1).toString().padStart(2, '0')}`;
      expect(value).toBe(expected);
    });

    it('should render 12 rows in the monthly trend table', async () => {
      await browser.pause(600);
      const trendSection = await browser.$$('.report-section');
      // Second section is the trend
      const trend = trendSection[1];
      const rows = await trend.$$('tbody tr');
      expect(rows.length).toBe(12);
    });

    it('should still show the heading after loading (no crash)', async () => {
      await browser.pause(300);
      const heading = await browser.$('.heading');
      const text = await heading.getText();
      expect(text).toBe('Reports');
    });
  });
});

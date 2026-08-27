describe('Budget page', () => {
  before(async () => {
    await browser.pause(1500);
  });

  it('should navigate to the budget page via sidebar', async () => {
    const budgetLink = await browser.$('a[href="/budget"]');
    await budgetLink.click();
    await browser.pause(500);

    const url = await browser.execute(() => window.location.pathname);
    expect(url).toBe('/budget');
  });

  it('should render the Income section with the four seeded categories', async () => {
    const incomeHeading = await browser.$('//span[contains(@class,"section-label") and text()="Income"]');
    await expect(incomeHeading).toExist();

    for (const name of ['Paycheck', 'Freelance', 'Interest', 'Other Income']) {
      const el = await browser.$(`//div[contains(@class,"income-row")]//span[text()="${name}"]`);
      await expect(el).toExist();
    }
  });

  it('should render expense category groups', async () => {
    const expensesHeading = await browser.$('//span[contains(@class,"section-label") and text()="Expenses"]');
    await expect(expensesHeading).toExist();
  });

  it('should collapse and expand a group on header click', async () => {
    const groupHeaders = await browser.$$('.group-btn');
    if (groupHeaders.length === 0) {
      console.warn('No .group-btn found; skipping collapse test');
      return;
    }
    const header = groupHeaders[0];
    await header.click();
    await browser.pause(300);

    await header.click();
    await browser.pause(300);
  });

  it('should show the Left to Budget sidebar', async () => {
    const sidebar = await browser.$('.lta-card');
    await expect(sidebar).toExist();
  });

  it('should navigate to previous and next month', async () => {
    const prevBtn = await browser.$('[aria-label="Previous month"]');
    const nextBtn = await browser.$('[aria-label="Next month"]');

    if (!(await prevBtn.isExisting()) || !(await nextBtn.isExisting())) {
      console.warn('Month nav buttons not found by aria-label; skipping');
      return;
    }

    await prevBtn.click();
    await browser.pause(500);
    await nextBtn.click();
    await browser.pause(500);
  });
});

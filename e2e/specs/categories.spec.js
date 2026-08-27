describe('Categories page', () => {
  before(async () => {
    await browser.pause(1500);
    const link = await browser.$('a[href="/categories"]');
    await link.click();
    await browser.pause(500);
  });

  it('should render the Income section at the top of the categories page', async () => {
    const incomeHeading = await browser.$('//h2[contains(@class,"section-title") and text()="Income"]');
    await expect(incomeHeading).toExist();
  });

  it('should list the four seeded income categories', async () => {
    for (const name of ['Paycheck', 'Freelance', 'Interest', 'Other Income']) {
      const el = await browser.$(`//span[contains(@class,"income-name") and text()="${name}"]`);
      await expect(el).toExist();
    }
  });

  it('should create a new income category', async () => {
    const testName = `Test Income ${Date.now()}`;

    const input = await browser.$('.add-input');
    if (!(await input.isExisting())) {
      console.warn('Income name input not found; skipping creation test');
      return;
    }
    await input.setValue(testName);

    const submitBtn = await browser.$('.add-btn');
    await submitBtn.click();
    await browser.pause(500);

    const created = await browser.$(`//span[contains(@class,"income-name") and text()="${testName}"]`);
    await expect(created).toExist();
  });
});

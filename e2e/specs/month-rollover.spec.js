describe('Month rollover', () => {
  before(async () => {
    await browser.pause(1500);
    const link = await browser.$('a[href="/budget"]');
    await link.click();
    await browser.pause(800);
  });

  it('shows the Close Month button for the current unclosed month', async () => {
    const btn = await browser.$('.close-month-btn');
    await expect(btn).toExist();
  });

  it('opens the confirmation dialog when the button is clicked', async () => {
    const btn = await browser.$('.close-month-btn');
    await btn.click();
    await browser.pause(400);

    const title = await browser.$('[data-slot="dialog-title"]');
    await expect(title).toExist();
    const titleText = await title.getText();
    expect(titleText).toContain('August 2026');

    const desc = await browser.$('[data-slot="dialog-description"]');
    const descText = await desc.getText();
    expect(descText).toContain('carries its debt forward');
  });

  it('closes the dialog on Cancel without navigating', async () => {
    const footer = await browser.$('[data-slot="dialog-footer"]');
    const cancelBtn = await footer.$('button*=Cancel');
    await cancelBtn.click();
    await browser.pause(400);

    const title = await browser.$('[data-slot="dialog-title"]');
    await expect(title).not.toExist();

    const header = await browser.$('.month-title');
    const headerText = await header.getText();
    expect(headerText).toContain('August 2026');
  });

  it('closes the month and navigates to September on confirm', async () => {
    const btn = await browser.$('.close-month-btn');
    await btn.click();
    await browser.pause(400);

    const footer = await browser.$('[data-slot="dialog-footer"]');
    const buttons = await footer.$$('button');
    // last button is Confirm ("Close Month")
    const confirmBtn = buttons[buttons.length - 1];
    await confirmBtn.click();
    await browser.pause(1200);

    const header = await browser.$('.month-title');
    const headerText = await header.getText();
    expect(headerText).toContain('September 2026');
  });

  it('does not show Close Month button for a future month', async () => {
    // currently on 2026-09 which is after CURRENT_MONTH (2026-08)
    const btn = await browser.$('.close-month-btn');
    await expect(btn).not.toExist();
  });

  it('does not show Close Month button after navigating back to the closed month', async () => {
    const prevBtn = await browser.$('[aria-label="Previous month"]');
    await prevBtn.click();
    await browser.pause(800);

    const header = await browser.$('.month-title');
    const headerText = await header.getText();
    expect(headerText).toContain('August 2026');

    // is_closed = true: button must not appear
    const btn = await browser.$('.close-month-btn');
    await expect(btn).not.toExist();
  });
});

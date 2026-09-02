// currentMonth and nextMonth are discovered at runtime by walking backwards to the
// most recent unclosed month. The persistent DB means a prior test run may have
// already closed the system-clock month, so we read state from the UI instead of
// computing it from new Date().
let currentMonth = '';
let nextMonth = '';

describe('Month rollover', () => {
  before(async () => {
    await browser.pause(1500);
    const link = await browser.$('a[href="/budget"]');
    await link.click();
    await browser.pause(800);

    // Walk backwards from whatever month the app currently displays until we find
    // one that shows the Close Month button (past, unclosed). Stop after 24 months
    // to avoid an infinite loop if something unexpected is wrong.
    for (let i = 0; i < 24; i++) {
      const btn = await browser.$('.close-month-btn');
      if (await btn.isExisting()) break;
      const prevBtn = await browser.$('[aria-label="Previous month"]');
      if (!(await prevBtn.isExisting())) break;
      await prevBtn.click();
      await browser.pause(600);
    }

    // Extract "Month YYYY" from the header so currentMonth is always a clean substring.
    const header = await browser.$('.month-title');
    const fullText = await header.getText();
    const m = fullText.match(/(\w+)\s+(\d{4})/);
    currentMonth = `${m[1]} ${m[2]}`;

    // Compute nextMonth by advancing one calendar month from currentMonth.
    const monthNames = ['January', 'February', 'March', 'April', 'May', 'June',
                        'July', 'August', 'September', 'October', 'November', 'December'];
    const monthIdx = monthNames.indexOf(m[1]);
    const year = parseInt(m[2], 10);
    nextMonth = new Date(year, monthIdx + 1, 1).toLocaleString('en-US', {
      month: 'long',
      year: 'numeric',
    });
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
    expect(titleText).toContain(currentMonth);

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
    expect(headerText).toContain(currentMonth);
  });

  it('closes the month and navigates to the next month on confirm', async () => {
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
    expect(headerText).toContain(nextMonth);
  });

  it('does not show Close Month button after closing', async () => {
    // currently on nextMonth; button must not appear (future or already closed)
    const btn = await browser.$('.close-month-btn');
    await expect(btn).not.toExist();
  });

  it('does not show Close Month button after navigating back to the closed month', async () => {
    const prevBtn = await browser.$('[aria-label="Previous month"]');
    await prevBtn.click();
    await browser.pause(800);

    const header = await browser.$('.month-title');
    const headerText = await header.getText();
    expect(headerText).toContain(currentMonth);

    // is_closed = true: button must not appear
    const btn = await browser.$('.close-month-btn');
    await expect(btn).not.toExist();
  });
});

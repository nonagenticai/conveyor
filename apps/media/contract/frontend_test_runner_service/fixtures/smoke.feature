Feature: capture smoke
  Scenario: the page loads and the button works
    Given I open the capture page
    Then the title is "Conveyor capture page"
    When I click "Go"
    Then the status is "clicked"
